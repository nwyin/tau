pub mod bash;
pub mod document;
pub mod file_edit;
pub mod file_read;
pub mod file_write;
pub mod from_id;
pub mod glob;
pub mod grep;
pub mod hashline;
pub mod log;
pub mod py_repl;
pub mod query;
pub mod subagent;
pub mod thread;
pub mod todo;
pub mod web_fetch;
pub mod web_search;

use std::collections::HashMap;
use std::sync::Arc;

use agent::types::AgentTool;

use crate::config::EditMode;

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

/// Return all built-in tools as a list ready to pass to the agent (replace mode).
pub fn all_tools() -> Vec<Arc<dyn AgentTool>> {
    tools_for_edit_mode("replace")
}

/// Return tools based on the configured edit mode.
///
/// Both modes return the same tool names (`file_read`, `file_edit`).
/// The mode controls behavior, schema, and description internally.
pub fn tools_for_edit_mode(edit_mode: &str) -> Vec<Arc<dyn AgentTool>> {
    let mode = EditMode::parse(edit_mode);
    vec![
        BashTool::arc(),
        FileReadTool::arc(mode.clone()),
        FileEditTool::arc(mode),
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
///
/// The edit mode controls the behavior of `file_read` and `file_edit`
/// while keeping the same canonical names.
pub fn all_known_tools(edit_mode: &str) -> HashMap<String, Arc<dyn AgentTool>> {
    let mode = EditMode::parse(edit_mode);
    let mut map: HashMap<String, Arc<dyn AgentTool>> = HashMap::new();
    map.insert("bash".to_string(), BashTool::arc());
    map.insert("file_read".to_string(), FileReadTool::arc(mode.clone()));
    map.insert("file_edit".to_string(), FileEditTool::arc(mode));
    map.insert("file_write".to_string(), FileWriteTool::arc());
    map.insert("glob".to_string(), GlobTool::arc());
    map.insert("grep".to_string(), GrepTool::arc());
    map.insert("web_fetch".to_string(), WebFetchTool::arc());
    map.insert("web_search".to_string(), WebSearchTool::arc());
    map.insert("subagent".to_string(), SubagentTool::arc());
    map.insert("todo".to_string(), TodoTool::arc());
    map
}

/// Create orchestration tools (thread + query) that require runtime state.
///
/// These are separate from the static tool constructors because they need
/// an OrchestratorState, model, and API key resolver.
///
/// Returns the tools plus an `EventForwarderCell` that should be populated
/// after agent creation to enable inner-thread event forwarding.
pub fn orchestration_tools(
    orchestrator: Arc<agent::orchestrator::OrchestratorState>,
    get_api_key: Option<agent::types::GetApiKeyFn>,
    model: ai::types::Model,
    edit_mode: &str,
    model_slots: crate::config::ModelSlots,
) -> (Vec<Arc<dyn AgentTool>>, thread::EventForwarderCell) {
    let cell = thread::event_forwarder_cell();
    let thread_tool = ThreadTool::arc(
        orchestrator.clone(),
        get_api_key.clone(),
        model.clone(),
        edit_mode.to_string(),
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
    let py_repl_tool = py_repl::PyReplTool::arc(
        edit_mode.to_string(),
        thread_tool.clone(),
        query_tool.clone(),
        document_tool.clone(),
    );
    let tools = vec![
        thread_tool,
        query_tool,
        document_tool,
        log_tool,
        from_id_tool,
        py_repl_tool,
    ];
    (tools, cell)
}

/// Resolve an allowlist of tool names against the registry.
/// Returns the matching tools in order. Logs a warning for unknown names and omits them.
pub fn tools_from_allowlist(names: &[String], edit_mode: &str) -> Vec<Arc<dyn AgentTool>> {
    let registry = all_known_tools(edit_mode);
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
