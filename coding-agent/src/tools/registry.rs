use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use agent::types::AgentTool;
use serde_json::Value;

use super::{
    BashTool, FileEditTool, FileReadTool, FileWriteTool, GlobTool, GrepTool, SubagentTool,
    TodoTool, WebFetchTool, WebSearchTool,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolFamily {
    Direct,
    Orchestration,
    Completion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultToolPermission {
    Allow,
    Ask,
}

#[derive(Debug, Clone, Copy)]
pub struct ToolMetadata {
    pub name: &'static str,
    pub family: ToolFamily,
    pub default_enabled: bool,
    pub default_permission: DefaultToolPermission,
    summarize: fn(&Value) -> String,
}

impl ToolMetadata {
    pub fn summarize(&self, args: &Value) -> String {
        (self.summarize)(args)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DirectToolSpec {
    pub metadata: ToolMetadata,
    build: fn() -> Arc<dyn AgentTool>,
    build_with_cwd: fn(PathBuf) -> Arc<dyn AgentTool>,
}

impl DirectToolSpec {
    pub fn build(&self) -> Arc<dyn AgentTool> {
        (self.build)()
    }

    pub fn build_with_cwd(&self, cwd: PathBuf) -> Arc<dyn AgentTool> {
        (self.build_with_cwd)(cwd)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ToolRegistry;

impl ToolRegistry {
    pub fn new() -> Self {
        Self
    }

    pub fn direct_specs(&self) -> &'static [DirectToolSpec] {
        DIRECT_TOOL_SPECS
    }

    pub fn default_direct_specs(&self) -> impl Iterator<Item = &'static DirectToolSpec> {
        self.direct_specs()
            .iter()
            .filter(|spec| spec.metadata.default_enabled)
    }

    pub fn metadata(&self, name: &str) -> Option<&'static ToolMetadata> {
        self.direct_specs()
            .iter()
            .map(|spec| &spec.metadata)
            .chain(ORCHESTRATION_METADATA.iter())
            .chain(COMPLETION_METADATA.iter())
            .find(|metadata| metadata.name == name)
    }

    pub fn direct_spec(&self, name: &str) -> Option<&'static DirectToolSpec> {
        self.direct_specs()
            .iter()
            .find(|spec| spec.metadata.name == name)
    }

    pub fn default_tools(&self) -> Vec<Arc<dyn AgentTool>> {
        self.default_direct_specs()
            .map(DirectToolSpec::build)
            .collect()
    }

    pub fn all_known_tools(&self) -> HashMap<String, Arc<dyn AgentTool>> {
        self.direct_specs()
            .iter()
            .map(|spec| (spec.metadata.name.to_string(), spec.build()))
            .collect()
    }

    pub fn all_known_tools_with_cwd(&self, cwd: PathBuf) -> HashMap<String, Arc<dyn AgentTool>> {
        self.direct_specs()
            .iter()
            .map(|spec| {
                (
                    spec.metadata.name.to_string(),
                    spec.build_with_cwd(cwd.clone()),
                )
            })
            .collect()
    }

    pub fn tools_from_allowlist(&self, names: &[String]) -> Vec<Arc<dyn AgentTool>> {
        names
            .iter()
            .filter_map(|name| match self.direct_spec(name.as_str()) {
                Some(spec) => Some(spec.build()),
                None => {
                    eprintln!("Warning: unknown tool '{}', skipping", name);
                    None
                }
            })
            .collect()
    }

    pub fn tools_from_allowlist_with_cwd(
        &self,
        names: &[String],
        cwd: PathBuf,
    ) -> Vec<Arc<dyn AgentTool>> {
        names
            .iter()
            .filter_map(|name| match self.direct_spec(name.as_str()) {
                Some(spec) => Some(spec.build_with_cwd(cwd.clone())),
                None => {
                    eprintln!("Warning: unknown tool '{}', skipping", name);
                    None
                }
            })
            .collect()
    }

    pub fn default_permission(&self, name: &str) -> DefaultToolPermission {
        self.metadata(name)
            .map(|metadata| metadata.default_permission)
            .unwrap_or(DefaultToolPermission::Ask)
    }

    pub fn summarize(&self, name: &str, args: &Value) -> String {
        self.metadata(name)
            .map(|metadata| metadata.summarize(args))
            .unwrap_or_default()
    }

    pub fn capability_tools(&self, capability: &str) -> Option<Vec<String>> {
        let names = match capability {
            "read" => vec!["file_read", "grep", "glob"],
            "write" => vec!["file_read", "file_edit", "file_write"],
            "terminal" => vec!["bash"],
            "web" => vec!["web_fetch", "web_search"],
            "full" => vec![
                "bash",
                "file_read",
                "file_edit",
                "file_write",
                "glob",
                "grep",
                "web_fetch",
                "web_search",
            ],
            _ => return None,
        };
        Some(names.into_iter().map(String::from).collect())
    }
}

pub fn summarize_tool_call(tool_name: &str, args: &Value) -> String {
    ToolRegistry::new().summarize(tool_name, args)
}

const fn metadata(
    name: &'static str,
    family: ToolFamily,
    default_enabled: bool,
    default_permission: DefaultToolPermission,
    summarize: fn(&Value) -> String,
) -> ToolMetadata {
    ToolMetadata {
        name,
        family,
        default_enabled,
        default_permission,
        summarize,
    }
}

const fn direct_spec(
    name: &'static str,
    default_permission: DefaultToolPermission,
    build: fn() -> Arc<dyn AgentTool>,
    build_with_cwd: fn(PathBuf) -> Arc<dyn AgentTool>,
    summarize: fn(&Value) -> String,
) -> DirectToolSpec {
    DirectToolSpec {
        metadata: metadata(
            name,
            ToolFamily::Direct,
            true,
            default_permission,
            summarize,
        ),
        build,
        build_with_cwd,
    }
}

fn build_bash() -> Arc<dyn AgentTool> {
    BashTool::arc()
}
fn build_bash_with_cwd(cwd: PathBuf) -> Arc<dyn AgentTool> {
    BashTool::arc_with_cwd(cwd)
}
fn build_file_read() -> Arc<dyn AgentTool> {
    FileReadTool::arc()
}
fn build_file_read_with_cwd(cwd: PathBuf) -> Arc<dyn AgentTool> {
    FileReadTool::arc_with_cwd(cwd)
}
fn build_file_edit() -> Arc<dyn AgentTool> {
    FileEditTool::arc()
}
fn build_file_edit_with_cwd(cwd: PathBuf) -> Arc<dyn AgentTool> {
    FileEditTool::arc_with_cwd(cwd)
}
fn build_file_write() -> Arc<dyn AgentTool> {
    FileWriteTool::arc()
}
fn build_file_write_with_cwd(cwd: PathBuf) -> Arc<dyn AgentTool> {
    FileWriteTool::arc_with_cwd(cwd)
}
fn build_glob() -> Arc<dyn AgentTool> {
    GlobTool::arc()
}
fn build_glob_with_cwd(cwd: PathBuf) -> Arc<dyn AgentTool> {
    GlobTool::arc_with_cwd(cwd)
}
fn build_grep() -> Arc<dyn AgentTool> {
    GrepTool::arc()
}
fn build_grep_with_cwd(cwd: PathBuf) -> Arc<dyn AgentTool> {
    GrepTool::arc_with_cwd(cwd)
}
fn build_web_fetch() -> Arc<dyn AgentTool> {
    WebFetchTool::arc()
}
fn build_web_fetch_with_cwd(_cwd: PathBuf) -> Arc<dyn AgentTool> {
    WebFetchTool::arc()
}
fn build_web_search() -> Arc<dyn AgentTool> {
    WebSearchTool::arc()
}
fn build_web_search_with_cwd(_cwd: PathBuf) -> Arc<dyn AgentTool> {
    WebSearchTool::arc()
}
fn build_subagent() -> Arc<dyn AgentTool> {
    SubagentTool::arc()
}
fn build_subagent_with_cwd(_cwd: PathBuf) -> Arc<dyn AgentTool> {
    SubagentTool::arc()
}
fn build_todo() -> Arc<dyn AgentTool> {
    TodoTool::arc()
}
fn build_todo_with_cwd(_cwd: PathBuf) -> Arc<dyn AgentTool> {
    TodoTool::arc()
}

static DIRECT_TOOL_SPECS: &[DirectToolSpec] = &[
    direct_spec(
        "bash",
        DefaultToolPermission::Ask,
        build_bash,
        build_bash_with_cwd,
        summarize_bash,
    ),
    direct_spec(
        "file_read",
        DefaultToolPermission::Allow,
        build_file_read,
        build_file_read_with_cwd,
        summarize_path,
    ),
    direct_spec(
        "file_edit",
        DefaultToolPermission::Ask,
        build_file_edit,
        build_file_edit_with_cwd,
        summarize_path,
    ),
    direct_spec(
        "file_write",
        DefaultToolPermission::Ask,
        build_file_write,
        build_file_write_with_cwd,
        summarize_path,
    ),
    direct_spec(
        "glob",
        DefaultToolPermission::Allow,
        build_glob,
        build_glob_with_cwd,
        summarize_pattern,
    ),
    direct_spec(
        "grep",
        DefaultToolPermission::Allow,
        build_grep,
        build_grep_with_cwd,
        summarize_pattern,
    ),
    direct_spec(
        "web_fetch",
        DefaultToolPermission::Allow,
        build_web_fetch,
        build_web_fetch_with_cwd,
        summarize_url,
    ),
    direct_spec(
        "web_search",
        DefaultToolPermission::Allow,
        build_web_search,
        build_web_search_with_cwd,
        summarize_query,
    ),
    direct_spec(
        "subagent",
        DefaultToolPermission::Ask,
        build_subagent,
        build_subagent_with_cwd,
        summarize_task,
    ),
    direct_spec(
        "todo",
        DefaultToolPermission::Allow,
        build_todo,
        build_todo_with_cwd,
        summarize_todo,
    ),
];

static ORCHESTRATION_METADATA: &[ToolMetadata] = &[
    metadata(
        "thread",
        ToolFamily::Orchestration,
        true,
        DefaultToolPermission::Ask,
        summarize_thread,
    ),
    metadata(
        "query",
        ToolFamily::Orchestration,
        true,
        DefaultToolPermission::Ask,
        summarize_query_tool,
    ),
    metadata(
        "document",
        ToolFamily::Orchestration,
        true,
        DefaultToolPermission::Ask,
        summarize_document,
    ),
    metadata(
        "log",
        ToolFamily::Orchestration,
        true,
        DefaultToolPermission::Ask,
        summarize_message,
    ),
    metadata(
        "from_id",
        ToolFamily::Orchestration,
        true,
        DefaultToolPermission::Ask,
        summarize_alias,
    ),
    metadata(
        "py_repl",
        ToolFamily::Orchestration,
        true,
        DefaultToolPermission::Ask,
        summarize_py_repl,
    ),
];

static COMPLETION_METADATA: &[ToolMetadata] = &[
    metadata(
        "complete",
        ToolFamily::Completion,
        false,
        DefaultToolPermission::Ask,
        summarize_result,
    ),
    metadata(
        "abort",
        ToolFamily::Completion,
        false,
        DefaultToolPermission::Ask,
        summarize_reason,
    ),
    metadata(
        "escalate",
        ToolFamily::Completion,
        false,
        DefaultToolPermission::Ask,
        summarize_problem,
    ),
];

fn string_arg(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn preview(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        let suffix = if max > 3 { "..." } else { "" };
        let keep = max.saturating_sub(suffix.len());
        format!("{}{}", text.chars().take(keep).collect::<String>(), suffix)
    } else {
        text.to_string()
    }
}

fn first_line_preview(text: &str, max: usize) -> String {
    preview(text.lines().next().unwrap_or(text), max)
}

fn summarize_bash(args: &Value) -> String {
    args.get("command")
        .and_then(|v| v.as_str())
        .map(|s| first_line_preview(s, 80))
        .unwrap_or_default()
}

fn summarize_path(args: &Value) -> String {
    string_arg(args, "path")
}

fn summarize_pattern(args: &Value) -> String {
    string_arg(args, "pattern")
}

fn summarize_url(args: &Value) -> String {
    string_arg(args, "url")
}

fn summarize_query(args: &Value) -> String {
    string_arg(args, "query")
}

fn summarize_task(args: &Value) -> String {
    args.get("task")
        .and_then(|v| v.as_str())
        .map(|s| first_line_preview(s, 80))
        .unwrap_or_default()
}

fn summarize_thread(args: &Value) -> String {
    let alias = args
        .get("alias")
        .and_then(|v| v.as_str())
        .unwrap_or("thread");
    let task = args
        .get("task")
        .and_then(|v| v.as_str())
        .map(|s| first_line_preview(s, 60))
        .unwrap_or_default();
    let episodes = args
        .get("episodes")
        .and_then(|v| v.as_array())
        .filter(|a| !a.is_empty())
        .map(|a| {
            let names: Vec<&str> = a.iter().filter_map(|v| v.as_str()).collect();
            format!(" [episodes: {}]", names.join(", "))
        })
        .unwrap_or_default();
    format!("{}: {}{}", alias, task, episodes)
}

fn summarize_query_tool(args: &Value) -> String {
    let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
    let alias = args
        .get("alias")
        .and_then(|v| v.as_str())
        .map(|a| format!("[{}] ", a))
        .unwrap_or_default();
    format!("{}{}", alias, preview(prompt, 70))
}

fn summarize_document(args: &Value) -> String {
    let op = args
        .get("operation")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let size = args
        .get("content")
        .and_then(|v| v.as_str())
        .map(|c| format!(" ({} chars)", c.len()))
        .unwrap_or_default();
    if name.is_empty() {
        op.to_string()
    } else {
        format!("{} {}{}", op, name, size)
    }
}

fn summarize_message(args: &Value) -> String {
    args.get("message")
        .and_then(|v| v.as_str())
        .map(|s| preview(s, 80))
        .unwrap_or_default()
}

fn summarize_alias(args: &Value) -> String {
    string_arg(args, "alias")
}

fn summarize_py_repl(args: &Value) -> String {
    let code = args.get("code").and_then(|v| v.as_str()).unwrap_or("");
    format!("{} lines", code.lines().count())
}

fn summarize_todo(args: &Value) -> String {
    let todos = args.get("todos").and_then(|v| v.as_array());
    match todos {
        Some(arr) => {
            let done = arr
                .iter()
                .filter(|t| t.get("status").and_then(|s| s.as_str()) == Some("completed"))
                .count();
            format!("[{}/{}]", done, arr.len())
        }
        None => String::new(),
    }
}

fn summarize_result(args: &Value) -> String {
    args.get("result")
        .and_then(|v| v.as_str())
        .map(|s| preview(s, 80))
        .unwrap_or_default()
}

fn summarize_reason(args: &Value) -> String {
    args.get("reason")
        .and_then(|v| v.as_str())
        .map(|s| preview(s, 80))
        .unwrap_or_default()
}

fn summarize_problem(args: &Value) -> String {
    args.get("problem")
        .and_then(|v| v.as_str())
        .map(|s| preview(s, 80))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(tools: Vec<Arc<dyn AgentTool>>) -> Vec<String> {
        tools.iter().map(|tool| tool.name().to_string()).collect()
    }

    #[test]
    fn default_direct_tools_keep_existing_order() {
        assert_eq!(
            names(ToolRegistry::new().default_tools()),
            vec![
                "bash",
                "file_read",
                "file_edit",
                "file_write",
                "glob",
                "grep",
                "web_fetch",
                "web_search",
                "subagent",
                "todo",
            ]
        );
    }

    #[test]
    fn all_known_tool_keys_match_cwd_tool_keys() {
        let registry = ToolRegistry::new();
        let mut plain: Vec<String> = registry.all_known_tools().keys().cloned().collect();
        let mut cwd: Vec<String> = registry
            .all_known_tools_with_cwd(PathBuf::from("/tmp"))
            .keys()
            .cloned()
            .collect();
        plain.sort();
        cwd.sort();
        assert_eq!(plain, cwd);
    }

    #[test]
    fn allowlist_preserves_order_and_skips_unknowns() {
        let tools = ToolRegistry::new().tools_from_allowlist(&[
            "grep".to_string(),
            "unknown".to_string(),
            "bash".to_string(),
        ]);
        assert_eq!(names(tools), vec!["grep", "bash"]);
    }

    #[test]
    fn default_permissions_match_current_policy() {
        let registry = ToolRegistry::new();
        assert_eq!(
            registry.default_permission("file_read"),
            DefaultToolPermission::Allow
        );
        assert_eq!(
            registry.default_permission("web_search"),
            DefaultToolPermission::Allow
        );
        assert_eq!(
            registry.default_permission("todo"),
            DefaultToolPermission::Allow
        );
        assert_eq!(
            registry.default_permission("bash"),
            DefaultToolPermission::Ask
        );
        assert_eq!(
            registry.default_permission("subagent"),
            DefaultToolPermission::Ask
        );
        assert_eq!(
            registry.default_permission("unknown"),
            DefaultToolPermission::Ask
        );
    }

    #[test]
    fn summaries_cover_direct_orchestration_and_completion_tools() {
        let registry = ToolRegistry::new();
        assert_eq!(
            registry.summarize("bash", &serde_json::json!({"command": "echo hello"})),
            "echo hello"
        );
        assert_eq!(
            registry.summarize("file_read", &serde_json::json!({"path": "/tmp/a.rs"})),
            "/tmp/a.rs"
        );
        assert_eq!(
            registry.summarize(
                "thread",
                &serde_json::json!({"alias": "a", "task": "scan files"})
            ),
            "a: scan files"
        );
        assert_eq!(
            registry.summarize(
                "document",
                &serde_json::json!({"operation": "write", "name": "notes", "content": "abc"})
            ),
            "write notes (3 chars)"
        );
        assert_eq!(
            registry.summarize("py_repl", &serde_json::json!({"code": "a = 1\nb = 2"})),
            "2 lines"
        );
        assert_eq!(
            registry.summarize(
                "todo",
                &serde_json::json!({"todos": [
                    {"status": "completed"},
                    {"status": "pending"}
                ]})
            ),
            "[1/2]"
        );
        assert_eq!(
            registry.summarize("log", &serde_json::json!({"message": "작업 ".repeat(40)})),
            format!(
                "{}...",
                "작업 ".repeat(26).chars().take(77).collect::<String>()
            )
        );
    }
}
