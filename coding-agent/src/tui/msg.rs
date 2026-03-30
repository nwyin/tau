use agent::types::AgentEvent;

use crate::permissions::PromptResult;

/// Custom message types injected into the ruse Program event loop.
#[derive(Clone)]
pub enum TauMsg {
    /// An event from the agent (streaming text, tool execution, etc.)
    AgentEvent(AgentEvent),
    /// A permission request from the agent's tool execution.
    /// The resp_tx is used to send the user's decision back to the blocked agent thread.
    PermissionRequest {
        tool_name: String,
        description: String,
        resp_tx: std::sync::mpsc::Sender<PromptResult>,
    },
    /// Tick for spinner/streaming animation.
    SpinnerTick,
}
