pub mod catalog;
pub mod models;
pub mod providers;
pub mod stream;
pub mod types;

// Convenience re-exports used throughout the agent crate.
pub use providers::{complete, complete_simple, register_builtin_providers, stream as stream_fn, stream_simple};
pub use stream::{
    assistant_message_event_stream, AssistantMessageEventSender, AssistantMessageEventStream,
};
pub use types::{
    AssistantMessage, AssistantMessageEvent, ContentBlock, Context, Cost, Message, Model,
    ModelCost, SimpleStreamOptions, StopReason, StreamOptions, ThinkingBudgets, ThinkingLevel,
    Tool, ToolResultMessage, Usage, UserBlock, UserContent, UserMessage,
};
