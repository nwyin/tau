//! Mirrors: packages/agent/test/agent.test.ts
//! Unit tests for the Agent class.

mod common;
use common::*;

use agent::agent::{Agent, AgentOptions, AgentStateInit, QueueMode};
use agent::types::{AgentMessage, ThinkingLevel};
use ai::stream::assistant_message_event_stream;
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
        stream_fn: None,
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
async fn prompt_throws_when_already_streaming() {
    let sender = Arc::new(std::sync::Mutex::new(None));
    let sender_ref = Arc::clone(&sender);
    let agent = Arc::new(Agent::new(AgentOptions {
        stream_fn: Some(stream_fn_once(move |_model, _context, _options| {
            let (tx, stream) = assistant_message_event_stream();
            *sender_ref.lock().unwrap() = Some(tx);
            stream
        })),
        ..default_opts()
    }));

    let first_prompt = {
        let agent = Arc::clone(&agent);
        tokio::spawn(async move { agent.prompt("First message").await })
    };

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert!(agent.with_state(|s| s.is_streaming));

    let err = agent.prompt("Second message").await.unwrap_err();
    assert!(err.to_string().contains("already streaming"));

    let msg = mock_assistant_message("done");
    if let Some(tx) = sender.lock().unwrap().as_mut() {
        tx.push(ai::types::AssistantMessageEvent::Start {
            partial: msg.clone(),
        });
        tx.push(ai::types::AssistantMessageEvent::Done {
            reason: msg.stop_reason.clone(),
            message: msg,
        });
    }

    first_prompt.await.unwrap().unwrap();
}

#[tokio::test]
async fn continue_throws_when_already_streaming() {
    let sender = Arc::new(std::sync::Mutex::new(None));
    let sender_ref = Arc::clone(&sender);
    let agent = Arc::new(Agent::new(AgentOptions {
        stream_fn: Some(stream_fn_once(move |_model, _context, _options| {
            let (tx, stream) = assistant_message_event_stream();
            *sender_ref.lock().unwrap() = Some(tx);
            stream
        })),
        ..default_opts()
    }));

    let first_prompt = {
        let agent = Arc::clone(&agent);
        tokio::spawn(async move { agent.prompt("First message").await })
    };

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert!(agent.with_state(|s| s.is_streaming));

    let err = agent.continue_().await.unwrap_err();
    assert!(err.to_string().contains("already streaming"));

    let msg = mock_assistant_message("done");
    if let Some(tx) = sender.lock().unwrap().as_mut() {
        tx.push(ai::types::AssistantMessageEvent::Start {
            partial: msg.clone(),
        });
        tx.push(ai::types::AssistantMessageEvent::Done {
            reason: msg.stop_reason.clone(),
            message: msg,
        });
    }

    first_prompt.await.unwrap().unwrap();
}

// ---------------------------------------------------------------------------
// Continue semantics — requires mock stream_fn
// ---------------------------------------------------------------------------

#[tokio::test]
async fn continue_processes_queued_follow_up_after_assistant_turn() {
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(stream_fn_from_messages(vec![mock_assistant_message(
            "Processed",
        )])),
        ..default_opts()
    });

    agent.replace_messages(vec![
        user_message("Initial"),
        AgentMessage::Llm(ai::types::Message::Assistant(mock_assistant_message(
            "Initial response",
        ))),
    ]);
    agent.follow_up(user_message("Queued follow-up"));

    agent.continue_().await.unwrap();

    agent.with_state(|s| {
        assert!(s.messages.iter().any(|message| matches!(
            message,
            AgentMessage::Llm(ai::types::Message::User(msg))
                if matches!(&msg.content, ai::types::UserContent::Text(text) if text == "Queued follow-up")
        )));
        assert_eq!(s.messages.last().unwrap().role(), "assistant");
    });
}

#[tokio::test]
async fn continue_one_at_a_time_steering_from_assistant_tail() {
    let responses = vec![
        mock_assistant_message("Processed 1"),
        mock_assistant_message("Processed 2"),
    ];
    let agent = Agent::new(AgentOptions {
        steering_mode: Some(QueueMode::OneAtATime),
        stream_fn: Some(stream_fn_from_messages(responses)),
        ..default_opts()
    });

    agent.replace_messages(vec![
        user_message("Initial"),
        AgentMessage::Llm(ai::types::Message::Assistant(mock_assistant_message(
            "Initial response",
        ))),
    ]);

    agent.steer(user_message("Steering 1"));
    agent.steer(user_message("Steering 2"));

    agent.continue_().await.unwrap();

    agent.with_state(|s| {
        let recent_roles: Vec<_> = s
            .messages
            .iter()
            .rev()
            .take(4)
            .map(|m| m.role().to_string())
            .collect();
        assert_eq!(
            recent_roles.into_iter().rev().collect::<Vec<_>>(),
            vec!["user", "assistant", "user", "assistant"]
        );
    });
}

#[tokio::test]
async fn session_id_forwarded_to_stream_fn() {
    let seen = Arc::new(std::sync::Mutex::new(vec![]));
    let seen_ref = Arc::clone(&seen);
    let mut agent = Agent::new(AgentOptions {
        session_id: Some("session-abc".into()),
        stream_fn: Some(stream_fn_once(move |_model, _context, options| {
            seen_ref
                .lock()
                .unwrap()
                .push(options.and_then(|opts| opts.base.session_id.clone()));
            instant_stream(mock_assistant_message("ok"))
        })),
        ..default_opts()
    });

    agent.prompt("hello").await.unwrap();
    agent.set_session_id(Some("session-def".into()));
    agent.prompt("hello again").await.unwrap();

    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            Some("session-abc".to_string()),
            Some("session-def".to_string())
        ]
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_opts() -> AgentOptions {
    AgentOptions {
        initial_state: Some(AgentStateInit {
            model: Some(mock_model()),
            ..Default::default()
        }),
        convert_to_llm: None,
        transform_context: None,
        stream_fn: None,
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
