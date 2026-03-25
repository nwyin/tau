pub mod bash;
pub mod file_edit;
pub mod file_read;
pub mod file_write;
pub mod glob;
pub mod grep;
pub mod hashline;
pub mod pycfg;
pub mod pycg;
pub mod subagent;
pub mod todo;
pub mod web_fetch;
pub mod web_search;

use std::collections::HashMap;
use std::sync::Arc;

use agent::types::AgentTool;

use crate::config::EditMode;

pub use bash::BashTool;
pub use file_edit::FileEditTool;
pub use file_read::FileReadTool;
pub use file_write::FileWriteTool;
pub use glob::GlobTool;
pub use grep::GrepTool;
pub use pycfg::{CfgFunctionsTool, CfgGraphTool, CfgSummaryTool};
pub use pycg::{
    CgCalleesTool, CgCallersTool, CgNeighborsTool, CgPathTool, CgSummaryTool, CgSymbolsTool,
};
pub use subagent::SubagentTool;
pub use todo::TodoTool;
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
    map.insert("cfg_functions".to_string(), CfgFunctionsTool::arc());
    map.insert("cfg_summary".to_string(), CfgSummaryTool::arc());
    map.insert("cfg_graph".to_string(), CfgGraphTool::arc());
    map.insert("cg_symbols".to_string(), CgSymbolsTool::arc());
    map.insert("cg_callers".to_string(), CgCallersTool::arc());
    map.insert("cg_callees".to_string(), CgCalleesTool::arc());
    map.insert("cg_path".to_string(), CgPathTool::arc());
    map.insert("cg_neighbors".to_string(), CgNeighborsTool::arc());
    map.insert("cg_summary".to_string(), CgSummaryTool::arc());
    map.insert("web_fetch".to_string(), WebFetchTool::arc());
    map.insert("web_search".to_string(), WebSearchTool::arc());
    map.insert("subagent".to_string(), SubagentTool::arc());
    map.insert("todo".to_string(), TodoTool::arc());
    map
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
