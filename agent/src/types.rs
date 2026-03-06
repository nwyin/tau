use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use ai::stream::AssistantMessageEventStream;
use ai::types::{
    AssistantMessageEvent, Context as LlmContext, Message, Model, SimpleStreamOptions,
    ToolResultMessage, UserBlock,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// ThinkingLevel — superset of ai::ThinkingLevel, adds "off"
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingLevel {
    #[default]
    Off,
    Minimal,
    Low,
    Medium,
    High,
    #[serde(rename = "xhigh")]
    XHigh,
}

impl ThinkingLevel {
    /// Convert to ai::ThinkingLevel, returning None for Off.
    pub fn to_ai(&self) -> Option<ai::types::ThinkingLevel> {
        match self {
            ThinkingLevel::Off => None,
            ThinkingLevel::Minimal => Some(ai::types::ThinkingLevel::Minimal),
            ThinkingLevel::Low => Some(ai::types::ThinkingLevel::Low),
            ThinkingLevel::Medium => Some(ai::types::ThinkingLevel::Medium),
            ThinkingLevel::High => Some(ai::types::ThinkingLevel::High),
            ThinkingLevel::XHigh => Some(ai::types::ThinkingLevel::XHigh),
        }
    }
}

// ---------------------------------------------------------------------------
// AgentMessage — Message extended with a custom escape hatch
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum AgentMessage {
    /// Standard LLM message (user / assistant / toolResult).
    Llm(Message),
    /// Application-defined message that is invisible to the LLM.
    Custom { role: String, data: Value },
}

impl AgentMessage {
    pub fn role(&self) -> &str {
        match self {
            AgentMessage::Llm(m) => m.role(),
            AgentMessage::Custom { role, .. } => role,
        }
    }

    pub fn as_message(&self) -> Option<&Message> {
        match self {
            AgentMessage::Llm(m) => Some(m),
            AgentMessage::Custom { .. } => None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        AgentMessage::Llm(Message::User(ai::types::UserMessage::new(content)))
    }
}

// ---------------------------------------------------------------------------
// AgentTool
// ---------------------------------------------------------------------------

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

#[derive(Debug)]
pub struct AgentToolResult {
    pub content: Vec<UserBlock>,
    pub details: Option<Value>,
}

pub type ToolUpdateFn = Arc<dyn Fn(AgentToolResult) + Send + Sync>;

pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;
    fn label(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> &Value;

    fn execute(
        &self,
        tool_call_id: String,
        params: Value,
        signal: Option<tokio_util::sync::CancellationToken>,
        on_update: Option<ToolUpdateFn>,
    ) -> BoxFuture<anyhow::Result<AgentToolResult>>;
}

// ---------------------------------------------------------------------------
// AgentState
// ---------------------------------------------------------------------------

pub struct AgentState {
    pub system_prompt: String,
    pub model: Model,
    pub thinking_level: ThinkingLevel,
    pub tools: Vec<Arc<dyn AgentTool>>,
    pub messages: Vec<AgentMessage>,
    pub is_streaming: bool,
    pub stream_message: Option<AgentMessage>,
    pub pending_tool_calls: std::collections::HashSet<String>,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// AgentContext — snapshot passed into each loop invocation
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AgentContext {
    pub system_prompt: String,
    pub messages: Vec<AgentMessage>,
    pub tools: Vec<Arc<dyn AgentTool>>,
}

// ---------------------------------------------------------------------------
// AgentLoopConfig
// ---------------------------------------------------------------------------

pub type ConvertToLlmFn =
    Arc<dyn Fn(Vec<AgentMessage>) -> BoxFuture<anyhow::Result<Vec<Message>>> + Send + Sync>;

pub type TransformContextFn = Arc<
    dyn Fn(Vec<AgentMessage>, Option<tokio_util::sync::CancellationToken>) -> BoxFuture<Vec<AgentMessage>>
        + Send
        + Sync,
>;

pub type GetApiKeyFn =
    Arc<dyn Fn(String) -> BoxFuture<Option<String>> + Send + Sync>;

pub type GetMessagesFn = Arc<dyn Fn() -> BoxFuture<Vec<AgentMessage>> + Send + Sync>;

pub type StreamAssistantFn = Arc<
    dyn Fn(Model, LlmContext, Option<SimpleStreamOptions>) -> anyhow::Result<AssistantMessageEventStream>
        + Send
        + Sync,
>;

pub struct AgentLoopConfig {
    pub model: Model,
    pub simple_options: SimpleStreamOptions,
    pub convert_to_llm: ConvertToLlmFn,
    pub transform_context: Option<TransformContextFn>,
    pub stream_fn: Option<StreamAssistantFn>,
    pub get_api_key: Option<GetApiKeyFn>,
    pub get_steering_messages: Option<GetMessagesFn>,
    pub get_follow_up_messages: Option<GetMessagesFn>,
}

// ---------------------------------------------------------------------------
// AgentEvent — emitted for UI / observer updates
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum AgentEvent {
    // Agent lifecycle
    AgentStart,
    AgentEnd { messages: Vec<AgentMessage> },

    // Turn lifecycle
    TurnStart,
    TurnEnd { message: AgentMessage, tool_results: Vec<ToolResultMessage> },

    // Message lifecycle
    MessageStart { message: AgentMessage },
    MessageUpdate { message: AgentMessage, assistant_event: AssistantMessageEvent },
    MessageEnd { message: AgentMessage },

    // Tool execution lifecycle
    ToolExecutionStart { tool_call_id: String, tool_name: String, args: Value },
    ToolExecutionUpdate { tool_call_id: String, tool_name: String, args: Value, partial_result: AgentToolResult },
    ToolExecutionEnd { tool_call_id: String, tool_name: String, result: AgentToolResult, is_error: bool },
}

// Needed so AgentToolResult can live inside AgentEvent::ToolExecutionUpdate.
// We store only the content + details, which are Clone-able via the inner types.
impl Clone for AgentToolResult {
    fn clone(&self) -> Self {
        AgentToolResult {
            content: self.content.clone(),
            details: self.details.clone(),
        }
    }
}
