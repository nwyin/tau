pub mod bash;
pub mod file_read;
pub mod file_write;

use std::sync::Arc;

use agent::types::AgentTool;

pub use bash::BashTool;
pub use file_read::FileReadTool;
pub use file_write::FileWriteTool;

/// Return all built-in tools as a list ready to pass to the agent.
pub fn all_tools() -> Vec<Arc<dyn AgentTool>> {
    vec![BashTool::arc(), FileReadTool::arc(), FileWriteTool::arc()]
}
