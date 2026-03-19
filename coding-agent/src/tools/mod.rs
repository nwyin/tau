pub mod bash;
pub mod file_edit;
pub mod file_read;
pub mod file_write;
pub mod hash_file_edit;
pub mod hash_file_read;
pub mod hashline;

use std::sync::Arc;

use agent::types::AgentTool;

pub use bash::BashTool;
pub use file_edit::FileEditTool;
pub use file_read::FileReadTool;
pub use file_write::FileWriteTool;
pub use hash_file_edit::HashFileEditTool;
pub use hash_file_read::HashFileReadTool;

/// Return all built-in tools as a list ready to pass to the agent.
pub fn all_tools() -> Vec<Arc<dyn AgentTool>> {
    vec![
        BashTool::arc(),
        FileEditTool::arc(),
        FileReadTool::arc(),
        FileWriteTool::arc(),
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
        ],
        _ => all_tools(), // "replace" or unknown → existing tools
    }
}
