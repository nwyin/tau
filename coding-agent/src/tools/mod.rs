pub mod bash;
pub mod file_edit;
pub mod file_read;
pub mod file_write;
pub mod glob;
pub mod grep;
pub mod hash_file_edit;
pub mod hash_file_read;
pub mod hashline;
pub mod pycfg;
pub mod run_tests;

use std::collections::HashMap;
use std::sync::Arc;

use agent::types::AgentTool;

pub use bash::BashTool;
pub use file_edit::FileEditTool;
pub use file_read::FileReadTool;
pub use file_write::FileWriteTool;
pub use glob::GlobTool;
pub use grep::GrepTool;
pub use hash_file_edit::HashFileEditTool;
pub use hash_file_read::HashFileReadTool;
pub use pycfg::{CfgFunctionsTool, CfgGraphTool, CfgSummaryTool};
pub use run_tests::RunTestsTool;

/// Return all built-in tools as a list ready to pass to the agent.
pub fn all_tools() -> Vec<Arc<dyn AgentTool>> {
    vec![
        BashTool::arc(),
        FileEditTool::arc(),
        FileReadTool::arc(),
        FileWriteTool::arc(),
        GlobTool::arc(),
        GrepTool::arc(),
    ]
}

/// Return tools based on the configured edit mode.
pub fn tools_for_edit_mode(edit_mode: &str) -> Vec<Arc<dyn AgentTool>> {
    match edit_mode {
        "hashline" => vec![
            BashTool::arc(),
            HashFileReadTool::arc(),
            HashFileEditTool::arc(),
            FileWriteTool::arc(),
            GlobTool::arc(),
            GrepTool::arc(),
        ],
        _ => all_tools(), // "replace" or unknown → existing tools
    }
}

/// Returns all known tool implementations keyed by canonical name.
///
/// Canonical names are stable identifiers ("file_read", "file_edit") regardless of
/// edit_mode. The registry resolves them to the appropriate implementation:
/// - "hashline" → HashFileReadTool / HashFileEditTool
/// - "replace" (default) → FileReadTool / FileEditTool
pub fn all_known_tools(edit_mode: &str) -> HashMap<String, Arc<dyn AgentTool>> {
    let mut map: HashMap<String, Arc<dyn AgentTool>> = HashMap::new();
    map.insert("bash".to_string(), BashTool::arc());
    map.insert("file_write".to_string(), FileWriteTool::arc());
    map.insert("glob".to_string(), GlobTool::arc());
    map.insert("grep".to_string(), GrepTool::arc());
    match edit_mode {
        "hashline" => {
            map.insert("file_read".to_string(), HashFileReadTool::arc());
            map.insert("file_edit".to_string(), HashFileEditTool::arc());
        }
        _ => {
            map.insert("file_read".to_string(), FileReadTool::arc());
            map.insert("file_edit".to_string(), FileEditTool::arc());
        }
    }
    map.insert("cfg_functions".to_string(), CfgFunctionsTool::arc());
    map.insert("cfg_summary".to_string(), CfgSummaryTool::arc());
    map.insert("cfg_graph".to_string(), CfgGraphTool::arc());
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
