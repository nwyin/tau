//! Tests for --prompt non-interactive mode.

use std::sync::{Arc, Mutex};

use agent::types::{AgentEvent, StreamAssistantFn};
use agent::{Agent, AgentOptions, AgentStateInit};
use ai::stream::assistant_message_event_stream;
use ai::types::{AssistantMessage, ContentBlock, StopReason, Usage};
use clap::Parser;
use coding_agent::cli::Cli;
use coding_agent::tools::all_tools;
use serde_json::json;

// ---------------------------------------------------------------------------
// Helpers (duplicated from integration.rs to keep tests self-contained)
// ---------------------------------------------------------------------------

fn mock_model() -> ai::types::Model {
    ai::types::Model {
        id: "mock".into(),
        name: "mock".into(),
        api: "openai-responses".into(),
        provider: "openai".into(),
        base_url: "https://example.invalid".into(),
        reasoning: false,
        input: vec!["text".into()],
        cost: ai::types::ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 8192,
        max_tokens: 2048,
        headers: None,
        compat: None,
    }
}

fn instant_stream(msg: AssistantMessage) -> ai::stream::AssistantMessageEventStream {
    let (mut tx, stream) = assistant_message_event_stream();
    tokio::spawn(async move {
        tx.push(ai::types::AssistantMessageEvent::Start {
            partial: msg.clone(),
        });
        let reason = msg.stop_reason.clone();
        tx.push(ai::types::AssistantMessageEvent::Done {
            reason,
            message: msg,
        });
    });
    stream
}

fn stream_fn_from_messages(messages: Vec<AssistantMessage>) -> StreamAssistantFn {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let messages = Arc::new(messages);
    let index = Arc::new(AtomicUsize::new(0));
    Arc::new(move |_model, _ctx, _opts| {
        let i = index.fetch_add(1, Ordering::SeqCst);
        let msg = messages
            .get(i)
            .cloned()
            .unwrap_or_else(|| messages.last().cloned().expect("at least one mock message"));
        Ok(instant_stream(msg))
    })
}

fn tool_call_message(id: &str, tool_name: &str, args: serde_json::Value) -> AssistantMessage {
    let mut map = std::collections::HashMap::new();
    if let serde_json::Value::Object(obj) = args {
        for (k, v) in obj {
            map.insert(k, v);
        }
    }
    AssistantMessage {
        role: "assistant".into(),
        content: vec![ContentBlock::ToolCall {
            id: id.into(),
            name: tool_name.into(),
            arguments: map,
            thought_signature: None,
        }],
        api: "openai-responses".into(),
        provider: "openai".into(),
        model: "mock".into(),
        usage: Usage::default(),
        stop_reason: StopReason::ToolUse,
        error_message: None,
        timestamp: 0,
    }
}

fn text_message(text: &str) -> AssistantMessage {
    AssistantMessage {
        role: "assistant".into(),
        content: vec![ContentBlock::Text {
            text: text.into(),
            text_signature: None,
        }],
        api: "openai-responses".into(),
        provider: "openai".into(),
        model: "mock".into(),
        usage: Usage::default(),
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 0,
    }
}

// ---------------------------------------------------------------------------
// CLI argument parsing tests
// ---------------------------------------------------------------------------

/// INV-2 (partial): Without --prompt, CLI prompt field is None.
#[test]
fn test_cli_no_prompt_flag() {
    let cli = Cli::try_parse_from(["coding-agent"]).unwrap();
    assert!(
        cli.prompt.is_none(),
        "expected no prompt when flag is absent"
    );
}

/// --prompt sets the prompt field.
#[test]
fn test_cli_prompt_flag_sets_value() {
    let cli = Cli::try_parse_from(["coding-agent", "--prompt", "hello world"]).unwrap();
    assert_eq!(cli.prompt.as_deref(), Some("hello world"));
}

/// -p shorthand works.
#[test]
fn test_cli_prompt_shorthand() {
    let cli = Cli::try_parse_from(["coding-agent", "-p", "test"]).unwrap();
    assert_eq!(cli.prompt.as_deref(), Some("test"));
}

/// --model flag sets the model field.
#[test]
fn test_cli_model_flag() {
    let cli = Cli::try_parse_from(["coding-agent", "--model", "gpt-4o"]).unwrap();
    assert_eq!(cli.model.as_deref(), Some("gpt-4o"));
}

/// -m shorthand works.
#[test]
fn test_cli_model_shorthand() {
    let cli = Cli::try_parse_from(["coding-agent", "-m", "gpt-4o"]).unwrap();
    assert_eq!(cli.model.as_deref(), Some("gpt-4o"));
}

/// --system-prompt flag sets the system_prompt field.
#[test]
fn test_cli_system_prompt_flag() {
    let cli = Cli::try_parse_from(["coding-agent", "--system-prompt", "Be helpful."]).unwrap();
    assert_eq!(cli.system_prompt.as_deref(), Some("Be helpful."));
}

/// INV-3: --prompt "-" is accepted as a literal "-" value (triggering stdin read).
#[test]
fn test_cli_prompt_stdin_marker() {
    let cli = Cli::try_parse_from(["coding-agent", "--prompt", "-"]).unwrap();
    assert_eq!(
        cli.prompt.as_deref(),
        Some("-"),
        "'-' should be accepted as the stdin marker"
    );
}

/// All three flags can be combined.
#[test]
fn test_cli_all_flags_combined() {
    let cli = Cli::try_parse_from([
        "coding-agent",
        "-p",
        "do something",
        "-m",
        "gpt-4o",
        "--system-prompt",
        "Be terse.",
    ])
    .unwrap();
    assert_eq!(cli.prompt.as_deref(), Some("do something"));
    assert_eq!(cli.model.as_deref(), Some("gpt-4o"));
    assert_eq!(cli.system_prompt.as_deref(), Some("Be terse."));
}

// ---------------------------------------------------------------------------
// INV-3: stdin reading via resolve_prompt_text_from
// ---------------------------------------------------------------------------

/// When prompt_arg is "-", content is read from the supplied reader.
#[test]
fn test_resolve_prompt_text_reads_stdin() {
    let input = b"list files in /tmp\n";
    let mut reader = std::io::Cursor::new(input);
    let result = coding_agent::resolve_prompt_text_from("-", &mut reader).unwrap();
    assert_eq!(result, "list files in /tmp");
}

/// When prompt_arg is "-" and reader is empty, result is an empty string.
#[test]
fn test_resolve_prompt_text_empty_stdin() {
    let mut reader = std::io::Cursor::new(b"");
    let result = coding_agent::resolve_prompt_text_from("-", &mut reader).unwrap();
    assert_eq!(result, "");
}

/// When prompt_arg is not "-", it is returned as-is (no reader consulted).
#[test]
fn test_resolve_prompt_text_literal() {
    let mut reader = std::io::Cursor::new(b"should not be read");
    let result = coding_agent::resolve_prompt_text_from("echo hello", &mut reader).unwrap();
    assert_eq!(result, "echo hello");
}

/// Leading/trailing whitespace is trimmed when reading from stdin.
#[test]
fn test_resolve_prompt_text_trims_whitespace() {
    let input = b"  trimmed prompt  \n";
    let mut reader = std::io::Cursor::new(input);
    let result = coding_agent::resolve_prompt_text_from("-", &mut reader).unwrap();
    assert_eq!(result, "trimmed prompt");
}

// ---------------------------------------------------------------------------
// INV-1: Non-interactive prompt mode runs to completion (mock stream)
// ---------------------------------------------------------------------------

/// In prompt mode, agent receives the prompt, executes tool calls, and
/// emits AgentEnd — verified without any real API calls.
#[tokio::test]
async fn test_prompt_mode_runs_to_completion() {
    // First LLM call: request bash "echo hello"
    // Second LLM call: final answer after seeing tool result
    let stream_fn = stream_fn_from_messages(vec![
        tool_call_message("c1", "bash", json!({"command": "echo hello"})),
        text_message("Done."),
    ]);

    let agent_ended = Arc::new(Mutex::new(false));
    let agent_ended_clone = Arc::clone(&agent_ended);

    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentStateInit {
            model: Some(mock_model()),
            system_prompt: Some("Be helpful.".into()),
            tools: Some(all_tools()),
            thinking_level: None,
        }),
        convert_to_llm: None,
        transform_context: None,
        stream_fn: Some(stream_fn),
        steering_mode: None,
        follow_up_mode: None,
        session_id: None,
        get_api_key: None,
        thinking_budgets: None,
        transport: None,
        max_retry_delay_ms: None,
        max_turns: None,
    });

    let _unsub = agent.subscribe(move |event| {
        if matches!(event, AgentEvent::AgentEnd { .. }) {
            *agent_ended_clone.lock().unwrap() = true;
        }
    });

    // Simulate prompt mode: call once and wait (no REPL loop)
    let result = agent.prompt("echo hello using bash").await;

    assert!(
        result.is_ok(),
        "agent.prompt() should return Ok in prompt mode"
    );
    assert!(
        *agent_ended.lock().unwrap(),
        "AgentEnd event must be received (agent ran to completion)"
    );
}

/// Prompt mode with a plain text response (no tool calls) also completes cleanly.
#[tokio::test]
async fn test_prompt_mode_no_tool_calls() {
    let stream_fn = stream_fn_from_messages(vec![text_message("Hello!")]);

    let agent_ended = Arc::new(Mutex::new(false));
    let agent_ended_clone = Arc::clone(&agent_ended);

    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentStateInit {
            model: Some(mock_model()),
            system_prompt: Some("Be helpful.".into()),
            tools: Some(all_tools()),
            thinking_level: None,
        }),
        convert_to_llm: None,
        transform_context: None,
        stream_fn: Some(stream_fn),
        steering_mode: None,
        follow_up_mode: None,
        session_id: None,
        get_api_key: None,
        thinking_budgets: None,
        transport: None,
        max_retry_delay_ms: None,
        max_turns: None,
    });

    let _unsub = agent.subscribe(move |event| {
        if matches!(event, AgentEvent::AgentEnd { .. }) {
            *agent_ended_clone.lock().unwrap() = true;
        }
    });

    let result = agent.prompt("say hello").await;

    assert!(result.is_ok());
    assert!(
        *agent_ended.lock().unwrap(),
        "AgentEnd must fire even without tool calls"
    );
}
