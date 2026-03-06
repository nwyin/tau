//! Mirrors: packages/agent/test/agent.test.ts
//! Unit tests for the Agent class.
//!
//! Tests that require a mock stream function are #[ignore] until stream_fn
//! injection is wired into Agent / AgentLoopConfig.

mod common;
use common::*;

use agent::agent::{Agent, AgentOptions, AgentStateInit, QueueMode};
use agent::types::{AgentMessage, ThinkingLevel};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// State initialization
// ---------------------------------------------------------------------------

#[test]
fn creates_agent_with_default_state() {
    // Agent::new requires a model — we supply one explicitly.
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentStateInit {
            model: Some(mock_model()),
            ..Default::default()
        }),
        convert_to_llm: None,
        transform_context: None,
        steering_mode: None,
        follow_up_mode: None,
        session_id: None,
        get_api_key: None,
        thinking_budgets: None,
        transport: None,
        max_retry_delay_ms: None,
    });

    agent.with_state(|s| {
        assert_eq!(s.system_prompt, "");
        assert_eq!(s.thinking_level, ThinkingLevel::Off);
        assert!(s.tools.is_empty());
        assert!(s.messages.is_empty());
        assert!(!s.is_streaming);
        assert!(s.stream_message.is_none());
        assert!(s.pending_tool_calls.is_empty());
        assert!(s.error.is_none());
    });
}

#[test]
fn creates_agent_with_custom_initial_state() {
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentStateInit {
            system_prompt: Some("You are a helpful assistant.".into()),
            model: Some(mock_model()),
            thinking_level: Some(ThinkingLevel::Low),
            tools: None,
        }),
        ..default_opts()
    });

    agent.with_state(|s| {
        assert_eq!(s.system_prompt, "You are a helpful assistant.");
        assert_eq!(s.thinking_level, ThinkingLevel::Low);
    });
}

// ---------------------------------------------------------------------------
// State mutators
// ---------------------------------------------------------------------------

#[test]
fn set_system_prompt() {
    let agent = make_agent();
    agent.set_system_prompt("Custom prompt");
    agent.with_state(|s| assert_eq!(s.system_prompt, "Custom prompt"));
}

#[test]
fn set_thinking_level() {
    let agent = make_agent();
    agent.set_thinking_level(ThinkingLevel::High);
    agent.with_state(|s| assert_eq!(s.thinking_level, ThinkingLevel::High));
}

#[test]
fn replace_messages() {
    let agent = make_agent();
    let msg = user_message("hello");
    agent.replace_messages(vec![msg.clone()]);
    agent.with_state(|s| {
        assert_eq!(s.messages.len(), 1);
        assert_eq!(s.messages[0].role(), "user");
    });
}

#[test]
fn append_message() {
    let agent = make_agent();
    agent.append_message(user_message("hello"));
    agent.append_message(user_message("world"));
    agent.with_state(|s| assert_eq!(s.messages.len(), 2));
}

// ---------------------------------------------------------------------------
// Queue management
// ---------------------------------------------------------------------------

#[test]
fn steer_queues_message_without_adding_to_state() {
    let agent = make_agent();
    let msg = user_message("steering message");
    agent.steer(msg);
    // The message is queued, not yet in state.messages
    agent.with_state(|s| assert!(s.messages.is_empty()));
    assert!(agent.has_queued_messages());
}

#[test]
fn follow_up_queues_message_without_adding_to_state() {
    let agent = make_agent();
    agent.follow_up(user_message("follow-up"));
    agent.with_state(|s| assert!(s.messages.is_empty()));
    assert!(agent.has_queued_messages());
}

#[test]
fn clear_all_queues() {
    let agent = make_agent();
    agent.steer(user_message("steer"));
    agent.follow_up(user_message("follow"));
    assert!(agent.has_queued_messages());
    agent.clear_all_queues();
    assert!(!agent.has_queued_messages());
}

// ---------------------------------------------------------------------------
// Subscribe / unsubscribe
// ---------------------------------------------------------------------------

#[test]
fn subscribe_does_not_fire_on_subscribe() {
    let agent = make_agent();
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count2 = Arc::clone(&count);
    let _unsub = agent.subscribe(move |_| {
        count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    });
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn state_mutators_do_not_emit_events() {
    let agent = make_agent();
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count2 = Arc::clone(&count);
    let _unsub = agent.subscribe(move |_| {
        count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    });

    agent.set_system_prompt("test");
    agent.set_thinking_level(ThinkingLevel::Medium);
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 0);
}

// ---------------------------------------------------------------------------
// Abort
// ---------------------------------------------------------------------------

#[test]
fn abort_does_not_panic_when_not_streaming() {
    let agent = make_agent();
    agent.abort(); // should not panic
}

// ---------------------------------------------------------------------------
// Prompt while streaming — requires mock stream_fn
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "needs stream_fn injection on Agent"]
async fn prompt_throws_when_already_streaming() {
    // Start a long-running prompt (backed by a mock stream that never resolves),
    // then call prompt() again — should return an error.
    // After asserting the error, abort and wait for the first prompt.
    todo!("needs stream_fn injection")
}

#[tokio::test]
#[ignore = "needs stream_fn injection on Agent"]
async fn continue_throws_when_already_streaming() {
    todo!("needs stream_fn injection")
}

// ---------------------------------------------------------------------------
// Continue semantics — requires mock stream_fn
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "needs stream_fn injection on Agent"]
async fn continue_processes_queued_follow_up_after_assistant_turn() {
    // Agent has user+assistant in messages, follow_up queued.
    // continue() should process the follow-up and produce a new assistant message.
    todo!("needs stream_fn injection")
}

#[tokio::test]
#[ignore = "needs stream_fn injection on Agent"]
async fn continue_one_at_a_time_steering_from_assistant_tail() {
    // Two steering messages queued.
    // continue() with one-at-a-time mode: each steering triggers exactly one LLM call.
    // Total: 2 LLM calls, 2 new assistant messages.
    todo!("needs stream_fn injection")
}

#[tokio::test]
#[ignore = "needs stream_fn injection on Agent"]
async fn session_id_forwarded_to_stream_fn() {
    // Agent constructed with session_id = "session-abc".
    // Mock stream_fn captures the session_id from options.
    // Assert it matches; then change it and assert the new value.
    todo!("needs stream_fn injection")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_opts() -> AgentOptions {
    AgentOptions {
        initial_state: Some(AgentStateInit { model: Some(mock_model()), ..Default::default() }),
        convert_to_llm: None,
        transform_context: None,
        steering_mode: None,
        follow_up_mode: None,
        session_id: None,
        get_api_key: None,
        thinking_budgets: None,
        transport: None,
        max_retry_delay_ms: None,
    }
}

fn make_agent() -> Agent {
    Agent::new(default_opts())
}
