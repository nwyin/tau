//! Mirrors: packages/agent/test/agent-loop.test.ts
//! Unit tests for agentLoop and agentLoopContinue using mock streams.
//!
//! NOTE: These tests require a `stream_fn` injection point on AgentLoopConfig
//! (analogous to the TS `streamFn` parameter). That plumbing is not yet
//! implemented — tests are marked #[ignore] until it is.

mod common;
use common::*;

use agent::types::{AgentContext, AgentEvent, AgentLoopConfig, AgentMessage};
use ai::types::{Message, StopReason};
use futures::StreamExt;
use std::sync::Arc;

fn identity_convert(messages: Vec<AgentMessage>) -> agent::types::BoxFuture<anyhow::Result<Vec<Message>>> {
    Box::pin(async move {
        Ok(messages.into_iter().filter_map(|m| m.as_message().cloned()).collect())
    })
}

fn base_config() -> AgentLoopConfig {
    AgentLoopConfig {
        model: mock_model(),
        simple_options: ai::types::SimpleStreamOptions::default(),
        convert_to_llm: Arc::new(|msgs| identity_convert(msgs)),
        transform_context: None,
        get_api_key: None,
        get_steering_messages: None,
        get_follow_up_messages: None,
    }
}

// ---------------------------------------------------------------------------
// agentLoop
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "needs stream_fn injection on AgentLoopConfig"]
async fn emits_events_with_agent_message_types() {
    let context = empty_context();
    let prompt = user_message("Hello");

    // TODO: inject mock stream function that returns instant_stream(mock_assistant_message("Hi there!"))
    // let stream = agent::loop_::agent_loop(vec![prompt], context, Arc::new(base_config()), None);

    // let mut events = vec![];
    // while let Some(event) = stream.next().await { events.push(event); }
    // let messages = stream.result().await;

    // assert_eq!(messages.len(), 2);
    // assert_eq!(messages[0].role(), "user");
    // assert_eq!(messages[1].role(), "assistant");

    // let types: Vec<_> = events.iter().map(|e| event_type(e)).collect();
    // assert!(types.contains(&"agent_start"));
    // assert!(types.contains(&"turn_start"));
    // assert!(types.contains(&"message_start"));
    // assert!(types.contains(&"message_end"));
    // assert!(types.contains(&"turn_end"));
    // assert!(types.contains(&"agent_end"));
    todo!("needs stream_fn injection")
}

#[tokio::test]
#[ignore = "needs stream_fn injection on AgentLoopConfig"]
async fn custom_message_types_filtered_by_convert_to_llm() {
    // Custom "notification" messages should be filtered out in convert_to_llm
    // and never sent to the LLM.
    todo!("needs stream_fn injection")
}

#[tokio::test]
#[ignore = "needs stream_fn injection on AgentLoopConfig"]
async fn transform_context_applied_before_convert_to_llm() {
    // transformContext should prune messages before convertToLlm sees them.
    // After pruning to last 2, convertToLlm should only receive 2 messages.
    todo!("needs stream_fn injection")
}

#[tokio::test]
#[ignore = "needs stream_fn injection on AgentLoopConfig"]
async fn handles_tool_calls_and_results() {
    // First LLM response: tool call to "echo" with value "hello"
    // Tool executes: records "hello"
    // Second LLM response: final text "done"
    // Assertions: tool was executed, tool_execution_start/end events emitted, is_error=false
    todo!("needs stream_fn injection")
}

#[tokio::test]
#[ignore = "needs stream_fn injection on AgentLoopConfig"]
async fn injects_steering_messages_and_skips_remaining_tool_calls() {
    // First LLM: two tool calls ("first", "second")
    // After first tool executes: steering message injected
    // Second tool: skipped (is_error=true, "Skipped due to queued user message")
    // Steering message appears in context on second LLM call
    todo!("needs stream_fn injection")
}

// ---------------------------------------------------------------------------
// agentLoopContinue
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Cannot continue: no messages in context")]
fn throws_when_context_has_no_messages() {
    let context = empty_context(); // no messages
    let config = Arc::new(base_config());
    agent::loop_::agent_loop_continue(context, config, None);
}

#[tokio::test]
#[ignore = "needs stream_fn injection on AgentLoopConfig"]
async fn continue_from_context_without_user_message_events() {
    // Context has one user message already.
    // agentLoopContinue should NOT emit message_start for the existing user message.
    // Only the new assistant message should appear in events.
    todo!("needs stream_fn injection")
}

#[tokio::test]
#[ignore = "needs stream_fn injection on AgentLoopConfig"]
async fn continue_allows_custom_last_message_converted_by_convert_to_llm() {
    // A custom message type as the last message in context.
    // convertToLlm converts it to a user message.
    // Should not throw, should produce an assistant response.
    todo!("needs stream_fn injection")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn event_type(e: &AgentEvent) -> &'static str {
    match e {
        AgentEvent::AgentStart => "agent_start",
        AgentEvent::AgentEnd { .. } => "agent_end",
        AgentEvent::TurnStart => "turn_start",
        AgentEvent::TurnEnd { .. } => "turn_end",
        AgentEvent::MessageStart { .. } => "message_start",
        AgentEvent::MessageUpdate { .. } => "message_update",
        AgentEvent::MessageEnd { .. } => "message_end",
        AgentEvent::ToolExecutionStart { .. } => "tool_execution_start",
        AgentEvent::ToolExecutionUpdate { .. } => "tool_execution_update",
        AgentEvent::ToolExecutionEnd { .. } => "tool_execution_end",
    }
}
