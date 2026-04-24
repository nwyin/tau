pub mod bash;
pub mod document;
pub mod file_edit;
pub mod file_read;
pub mod file_write;
pub mod from_id;
pub mod glob;
pub mod grep;
pub mod log;
pub mod py_repl;
pub mod query;
pub mod subagent;
pub mod thread;
pub mod todo;
pub mod web_fetch;
pub mod web_search;
pub mod worktree;

use std::collections::HashMap;
use std::sync::Arc;

use agent::types::AgentTool;

pub use bash::BashTool;
pub use document::DocumentTool;
pub use file_edit::FileEditTool;
pub use file_read::FileReadTool;
pub use file_write::FileWriteTool;
pub use from_id::FromIdTool;
pub use glob::GlobTool;
pub use grep::GrepTool;
pub use log::LogTool;
pub use query::QueryTool;
pub use subagent::SubagentTool;
pub use thread::ThreadTool;
pub use todo::{TodoItem, TodoTool};
pub use web_fetch::WebFetchTool;
pub use web_search::WebSearchTool;

/// Return all built-in tools.
pub fn default_tools() -> Vec<Arc<dyn AgentTool>> {
    vec![
        BashTool::arc(),
        FileReadTool::arc(),
        FileEditTool::arc(),
        FileWriteTool::arc(),
        GlobTool::arc(),
        GrepTool::arc(),
        WebFetchTool::arc(),
        WebSearchTool::arc(),
        SubagentTool::arc(),
        TodoTool::arc(),
    ]
}

/// Returns all known tool implementations keyed by canonical name.
pub fn all_known_tools() -> HashMap<String, Arc<dyn AgentTool>> {
    let mut map: HashMap<String, Arc<dyn AgentTool>> = HashMap::new();
    map.insert("bash".to_string(), BashTool::arc());
    map.insert("file_read".to_string(), FileReadTool::arc());
    map.insert("file_edit".to_string(), FileEditTool::arc());
    map.insert("file_write".to_string(), FileWriteTool::arc());
    map.insert("glob".to_string(), GlobTool::arc());
    map.insert("grep".to_string(), GrepTool::arc());
    map.insert("web_fetch".to_string(), WebFetchTool::arc());
    map.insert("web_search".to_string(), WebSearchTool::arc());
    map.insert("subagent".to_string(), SubagentTool::arc());
    map.insert("todo".to_string(), TodoTool::arc());
    map
}

pub struct OrchestrationToolSet {
    pub tools: Vec<Arc<dyn AgentTool>>,
    pub event_forwarder_cell: thread::EventForwarderCell,
    pub thread_tool: Arc<dyn AgentTool>,
    pub query_tool: Arc<dyn AgentTool>,
    pub document_tool: Arc<dyn AgentTool>,
}

/// Create orchestration tools except py_repl, which needs the final wrapped tool set.
pub fn orchestration_core_tools(
    orchestrator: Arc<agent::orchestrator::OrchestratorState>,
    get_api_key: Option<agent::types::GetApiKeyFn>,
    model: ai::types::Model,
    model_slots: crate::config::ModelSlots,
) -> OrchestrationToolSet {
    let cell = thread::event_forwarder_cell();
    let thread_tool = ThreadTool::arc(
        orchestrator.clone(),
        get_api_key.clone(),
        model.clone(),
        cell.clone(),
        model_slots.clone(),
    );
    let query_tool = QueryTool::arc(
        orchestrator.clone(),
        get_api_key.clone(),
        model.clone(),
        model_slots.clone(),
        cell.clone(),
    );
    let document_tool = DocumentTool::arc(orchestrator.clone(), cell.clone());
    let log_tool = LogTool::arc(orchestrator.clone());
    let from_id_tool = FromIdTool::arc(orchestrator);
    let tools = vec![
        thread_tool.clone(),
        query_tool.clone(),
        document_tool.clone(),
        log_tool,
        from_id_tool,
    ];
    OrchestrationToolSet {
        tools,
        event_forwarder_cell: cell,
        thread_tool,
        query_tool,
        document_tool,
    }
}

/// Create orchestration tools (thread + query) that require runtime state.
pub fn orchestration_tools(
    orchestrator: Arc<agent::orchestrator::OrchestratorState>,
    get_api_key: Option<agent::types::GetApiKeyFn>,
    model: ai::types::Model,
    model_slots: crate::config::ModelSlots,
) -> (Vec<Arc<dyn AgentTool>>, thread::EventForwarderCell) {
    let core = orchestration_core_tools(orchestrator, get_api_key, model, model_slots);
    let py_repl_tool = py_repl::PyReplTool::arc(
        core.thread_tool.clone(),
        core.query_tool.clone(),
        core.document_tool.clone(),
    );
    let mut tools = core.tools;
    tools.push(py_repl_tool);
    (tools, core.event_forwarder_cell)
}

/// Resolve an allowlist of tool names against the registry.
pub fn tools_from_allowlist(names: &[String]) -> Vec<Arc<dyn AgentTool>> {
    let registry = all_known_tools();
    names
        .iter()
        .filter_map(|name| match registry.get(name.as_str()) {
            Some(tool) => Some(Arc::clone(tool)),
            None => {
                eprintln!("Warning: unknown tool '{}', skipping", name);
                None
            }
        })
        .collect()
}

/// Returns all known tools with a custom working directory for filesystem tools.
pub fn all_known_tools_with_cwd(cwd: std::path::PathBuf) -> HashMap<String, Arc<dyn AgentTool>> {
    let mut map: HashMap<String, Arc<dyn AgentTool>> = HashMap::new();
    map.insert("bash".into(), BashTool::arc_with_cwd(cwd.clone()));
    map.insert("file_read".into(), FileReadTool::arc_with_cwd(cwd.clone()));
    map.insert("file_edit".into(), FileEditTool::arc_with_cwd(cwd.clone()));
    map.insert(
        "file_write".into(),
        FileWriteTool::arc_with_cwd(cwd.clone()),
    );
    map.insert("glob".into(), GlobTool::arc_with_cwd(cwd.clone()));
    map.insert("grep".into(), GrepTool::arc_with_cwd(cwd));
    map.insert("web_fetch".into(), WebFetchTool::arc());
    map.insert("web_search".into(), WebSearchTool::arc());
    map.insert("subagent".into(), SubagentTool::arc());
    map.insert("todo".into(), TodoTool::arc());
    map
}

/// Resolve an allowlist of tool names with a custom working directory.
pub fn tools_from_allowlist_with_cwd(
    names: &[String],
    cwd: std::path::PathBuf,
) -> Vec<Arc<dyn AgentTool>> {
    let registry = all_known_tools_with_cwd(cwd);
    names
        .iter()
        .filter_map(|name| match registry.get(name.as_str()) {
            Some(tool) => Some(Arc::clone(tool)),
            None => {
                eprintln!("Warning: unknown tool '{}', skipping", name);
                None
            }
        })
        .collect()
}
