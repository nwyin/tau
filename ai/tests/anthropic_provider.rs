//! Unit tests for the Anthropic Messages provider.
//!
//! Tests at the SSE event level — no live HTTP required.
//! Live API tests are marked #[ignore = "requires ANTHROPIC_API_KEY"].

mod common;
use common::{env_key, registry_lock};

use ai::models::get_model;
use ai::providers::anthropic::{
    build_request_body, convert_anthropic_messages, convert_anthropic_tools, map_stop_reason,
    process_anthropic_events, AnthropicRequestOptions,
};
use ai::providers::{clear_api_providers, complete_simple, register_builtin_providers};
use ai::stream::assistant_message_event_stream;
use ai::types::{
    AssistantMessage, ContentBlock, Context, Cost, Message, Model, ModelCost, SimpleStreamOptions,
    StopReason, ThinkingLevel, Tool, ToolResultMessage, Usage, UserBlock, UserContent, UserMessage,
};
use serde_json::json;
use std::collections::HashMap;

// =============================================================================
// Helpers
// =============================================================================

fn anthropic_model() -> Model {
    Model {
        id: "claude-3-5-haiku-20241022".into(),
        name: "Claude Haiku 3.5".into(),
        api: "anthropic-messages".into(),
        provider: "anthropic".into(),
        base_url: "https://api.anthropic.com".into(),
        reasoning: false,
        input: vec!["text".into(), "image".into()],
        cost: ModelCost {
            input: 0.8,
            output: 4.0,
            cache_read: 0.08,
            cache_write: 1.0,
        },
        context_window: 200_000,
        max_tokens: 8_192,
        headers: None,
        compat: None,
    }
}

fn reasoning_model() -> Model {
    Model {
        id: "claude-opus-4-6".into(),
        name: "Claude Opus 4.6".into(),
        api: "anthropic-messages".into(),
        provider: "anthropic".into(),
        base_url: "https://api.anthropic.com".into(),
        reasoning: true,
        input: vec!["text".into()],
        cost: ModelCost {
            input: 15.0,
            output: 75.0,
            cache_read: 1.5,
            cache_write: 18.75,
        },
        context_window: 200_000,
        max_tokens: 32_000,
        headers: None,
        compat: None,
    }
}

fn non_xhigh_reasoning_model() -> Model {
    Model {
        id: "claude-3-7-sonnet-20250219".into(),
        name: "Claude Sonnet 3.7".into(),
        api: "anthropic-messages".into(),
        provider: "anthropic".into(),
        base_url: "https://api.anthropic.com".into(),
        reasoning: true,
        input: vec!["text".into()],
        cost: ModelCost {
            input: 3.0,
            output: 15.0,
            cache_read: 0.3,
            cache_write: 3.75,
        },
        context_window: 200_000,
        max_tokens: 128_000,
        headers: None,
        compat: None,
    }
}

async fn run_anthropic_events(events: Vec<serde_json::Value>, model: &Model) -> AssistantMessage {
    let mut output = AssistantMessage {
        role: "assistant".into(),
        content: Vec::new(),
        api: model.api.clone(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        usage: Usage {
            cost: Cost::default(),
            ..Default::default()
        },
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 0,
    };

    let (mut tx, _stream) = assistant_message_event_stream();
    process_anthropic_events(events, &mut output, &mut tx, model)
        .await
        .unwrap();

    output
}

// =============================================================================
// INV-1: Message conversion preserves all user content through round-trip
// =============================================================================

#[test]
fn inv1_user_text_preserved() {
    let model = anthropic_model();
    let context = Context {
        system_prompt: None,
        messages: vec![Message::User(UserMessage {
            role: "user".into(),
            content: UserContent::Text("Hello Anthropic!".into()),
            timestamp: 0,
        })],
        tools: None,
    };
    let (sys, msgs) = convert_anthropic_messages(&model, &context);
    assert!(sys.is_none());
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["role"], "user");
    assert_eq!(msgs[0]["content"][0]["type"], "text");
    assert_eq!(msgs[0]["content"][0]["text"], "Hello Anthropic!");
}

#[test]
fn inv1_system_prompt_extracted() {
    let model = anthropic_model();
    let context = Context {
        system_prompt: Some("You are a coding assistant.".into()),
        messages: vec![],
        tools: None,
    };
    let (sys, msgs) = convert_anthropic_messages(&model, &context);
    assert_eq!(sys.as_deref(), Some("You are a coding assistant."));
    assert!(msgs.is_empty());
}

#[test]
fn inv1_user_image_preserved_for_image_capable_model() {
    let model = anthropic_model(); // supports image
    let context = Context {
        system_prompt: None,
        messages: vec![Message::User(UserMessage {
            role: "user".into(),
            content: UserContent::Blocks(vec![
                UserBlock::Text {
                    text: "Describe this:".into(),
                },
                UserBlock::Image {
                    data: "base64data==".into(),
                    mime_type: "image/png".into(),
                },
            ]),
            timestamp: 0,
        })],
        tools: None,
    };
    let (_sys, msgs) = convert_anthropic_messages(&model, &context);
    assert_eq!(msgs[0]["content"].as_array().unwrap().len(), 2);
    assert_eq!(msgs[0]["content"][1]["type"], "image");
    assert_eq!(msgs[0]["content"][1]["source"]["type"], "base64");
    assert_eq!(msgs[0]["content"][1]["source"]["media_type"], "image/png");
    assert_eq!(msgs[0]["content"][1]["source"]["data"], "base64data==");
}

#[test]
fn inv1_user_image_dropped_for_text_only_model() {
    let mut model = anthropic_model();
    model.input = vec!["text".into()]; // text only
    let context = Context {
        system_prompt: None,
        messages: vec![Message::User(UserMessage {
            role: "user".into(),
            content: UserContent::Blocks(vec![
                UserBlock::Text { text: "Hi".into() },
                UserBlock::Image {
                    data: "data".into(),
                    mime_type: "image/jpeg".into(),
                },
            ]),
            timestamp: 0,
        })],
        tools: None,
    };
    let (_sys, msgs) = convert_anthropic_messages(&model, &context);
    assert_eq!(msgs[0]["content"].as_array().unwrap().len(), 1);
    assert_eq!(msgs[0]["content"][0]["type"], "text");
}

// =============================================================================
// INV-2: Tool calls in assistant messages serialize correctly
// =============================================================================

#[test]
fn inv2_assistant_tool_call_becomes_tool_use_block() {
    let model = anthropic_model();
    let mut args = HashMap::new();
    args.insert("query".to_string(), json!("hello world"));

    let context = Context {
        system_prompt: None,
        messages: vec![Message::Assistant(AssistantMessage {
            role: "assistant".into(),
            content: vec![ContentBlock::ToolCall {
                id: "toolu_abc123".into(),
                name: "search".into(),
                arguments: args,
                thought_signature: None,
            }],
            api: "anthropic-messages".into(),
            provider: "anthropic".into(),
            model: "claude-3-5-haiku-20241022".into(),
            usage: Usage::default(),
            stop_reason: StopReason::ToolUse,
            error_message: None,
            timestamp: 0,
        })],
        tools: None,
    };
    let (_sys, msgs) = convert_anthropic_messages(&model, &context);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["role"], "assistant");
    let block = &msgs[0]["content"][0];
    assert_eq!(block["type"], "tool_use");
    assert_eq!(block["id"], "toolu_abc123");
    assert_eq!(block["name"], "search");
    assert_eq!(block["input"]["query"], "hello world");
}

#[test]
fn inv2_tool_result_becomes_user_message_with_tool_result_block() {
    let model = anthropic_model();
    let context = Context {
        system_prompt: None,
        messages: vec![Message::ToolResult(ToolResultMessage {
            role: "toolResult".into(),
            tool_call_id: "toolu_abc123".into(),
            tool_name: "search".into(),
            content: vec![UserBlock::Text {
                text: "result text".into(),
            }],
            details: None,
            is_error: false,
            timestamp: 0,
        })],
        tools: None,
    };
    let (_sys, msgs) = convert_anthropic_messages(&model, &context);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["role"], "user");
    let block = &msgs[0]["content"][0];
    assert_eq!(block["type"], "tool_result");
    assert_eq!(block["tool_use_id"], "toolu_abc123");
    assert_eq!(block["content"][0]["text"], "result text");
    assert!(block.get("is_error").is_none() || block["is_error"].is_null());
}

#[test]
fn inv2_error_tool_result_sets_is_error() {
    let model = anthropic_model();
    let context = Context {
        system_prompt: None,
        messages: vec![Message::ToolResult(ToolResultMessage {
            role: "toolResult".into(),
            tool_call_id: "toolu_xyz".into(),
            tool_name: "bash".into(),
            content: vec![UserBlock::Text {
                text: "Command not found".into(),
            }],
            details: None,
            is_error: true,
            timestamp: 0,
        })],
        tools: None,
    };
    let (_sys, msgs) = convert_anthropic_messages(&model, &context);
    assert_eq!(msgs[0]["content"][0]["is_error"], true);
}

#[test]
fn inv2_thinking_block_with_signature_preserved() {
    let model = anthropic_model();
    let context = Context {
        system_prompt: None,
        messages: vec![Message::Assistant(AssistantMessage {
            role: "assistant".into(),
            content: vec![ContentBlock::Thinking {
                thinking: "I should think about this".into(),
                thinking_signature: Some("sig_abc".into()),
                redacted: None,
            }],
            api: "anthropic-messages".into(),
            provider: "anthropic".into(),
            model: "claude-3-5-haiku-20241022".into(),
            usage: Usage::default(),
            stop_reason: StopReason::Stop,
            error_message: None,
            timestamp: 0,
        })],
        tools: None,
    };
    let (_sys, msgs) = convert_anthropic_messages(&model, &context);
    assert_eq!(msgs[0]["content"][0]["type"], "thinking");
    assert_eq!(
        msgs[0]["content"][0]["thinking"],
        "I should think about this"
    );
    assert_eq!(msgs[0]["content"][0]["signature"], "sig_abc");
}

#[test]
fn inv2_thinking_block_without_signature_skipped() {
    let model = anthropic_model();
    let context = Context {
        system_prompt: None,
        messages: vec![Message::Assistant(AssistantMessage {
            role: "assistant".into(),
            content: vec![
                ContentBlock::Thinking {
                    thinking: "thinking...".into(),
                    thinking_signature: None, // no signature → skip
                    redacted: None,
                },
                ContentBlock::Text {
                    text: "answer".into(),
                    text_signature: None,
                },
            ],
            api: "anthropic-messages".into(),
            provider: "anthropic".into(),
            model: "claude-3-5-haiku-20241022".into(),
            usage: Usage::default(),
            stop_reason: StopReason::Stop,
            error_message: None,
            timestamp: 0,
        })],
        tools: None,
    };
    let (_sys, msgs) = convert_anthropic_messages(&model, &context);
    // Only the text block should appear
    assert_eq!(msgs[0]["content"].as_array().unwrap().len(), 1);
    assert_eq!(msgs[0]["content"][0]["type"], "text");
}

#[test]
fn inv2_tool_results_merge_into_same_user_turn() {
    let model = anthropic_model();
    // Two consecutive tool results should end up in the same user message
    let context = Context {
        system_prompt: None,
        messages: vec![
            Message::ToolResult(ToolResultMessage {
                role: "toolResult".into(),
                tool_call_id: "toolu_1".into(),
                tool_name: "tool_a".into(),
                content: vec![UserBlock::Text {
                    text: "result a".into(),
                }],
                details: None,
                is_error: false,
                timestamp: 0,
            }),
            Message::ToolResult(ToolResultMessage {
                role: "toolResult".into(),
                tool_call_id: "toolu_2".into(),
                tool_name: "tool_b".into(),
                content: vec![UserBlock::Text {
                    text: "result b".into(),
                }],
                details: None,
                is_error: false,
                timestamp: 0,
            }),
        ],
        tools: None,
    };
    let (_sys, msgs) = convert_anthropic_messages(&model, &context);
    assert_eq!(
        msgs.len(),
        1,
        "tool results should merge into one user message"
    );
    assert_eq!(msgs[0]["content"].as_array().unwrap().len(), 2);
    assert_eq!(msgs[0]["content"][0]["tool_use_id"], "toolu_1");
    assert_eq!(msgs[0]["content"][1]["tool_use_id"], "toolu_2");
}

// =============================================================================
// INV-3: Stop reason mapping covers all Anthropic values without panic
// =============================================================================

#[test]
fn inv3_end_turn_is_stop() {
    assert_eq!(map_stop_reason(Some("end_turn")), StopReason::Stop);
}

#[test]
fn inv3_max_tokens_is_length() {
    assert_eq!(map_stop_reason(Some("max_tokens")), StopReason::Length);
}

#[test]
fn inv3_tool_use_is_tool_use() {
    assert_eq!(map_stop_reason(Some("tool_use")), StopReason::ToolUse);
}

#[test]
fn inv3_refusal_is_stop() {
    assert_eq!(map_stop_reason(Some("refusal")), StopReason::Stop);
}

#[test]
fn inv3_pause_turn_is_stop() {
    assert_eq!(map_stop_reason(Some("pause_turn")), StopReason::Stop);
}

#[test]
fn inv3_none_is_stop() {
    assert_eq!(map_stop_reason(None), StopReason::Stop);
}

#[test]
fn inv3_unknown_value_is_stop_no_panic() {
    assert_eq!(
        map_stop_reason(Some("some_future_reason")),
        StopReason::Stop
    );
}

// =============================================================================
// INV-4: Streaming event sequence produces valid AssistantMessage with usage
// =============================================================================

#[tokio::test]
async fn inv4_text_only_streaming() {
    let model = anthropic_model();
    let events = vec![
        json!({
            "type": "message_start",
            "message": {
                "id": "msg_01",
                "type": "message",
                "role": "assistant",
                "model": "claude-3-5-haiku-20241022",
                "content": [],
                "stop_reason": null,
                "usage": {
                    "input_tokens": 20,
                    "cache_read_input_tokens": 0,
                    "cache_creation_input_tokens": 0,
                    "output_tokens": 1
                }
            }
        }),
        json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "text", "text": "" } }),
        json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": "Hello " } }),
        json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": "world!" } }),
        json!({ "type": "content_block_stop", "index": 0 }),
        json!({
            "type": "message_delta",
            "delta": { "stop_reason": "end_turn", "stop_sequence": null },
            "usage": { "output_tokens": 3 }
        }),
        json!({ "type": "message_stop" }),
    ];

    let output = run_anthropic_events(events, &model).await;
    assert_eq!(output.stop_reason, StopReason::Stop);
    assert_eq!(output.usage.input, 20);
    assert_eq!(output.usage.output, 3);
    assert!(output
        .content
        .iter()
        .any(|b| matches!(b, ContentBlock::Text { text, .. } if text == "Hello world!")));
}

#[tokio::test]
async fn inv4_tool_call_streaming() {
    let model = anthropic_model();
    let events = vec![
        json!({
            "type": "message_start",
            "message": {
                "id": "msg_02",
                "type": "message",
                "role": "assistant",
                "model": "claude-3-5-haiku-20241022",
                "content": [],
                "stop_reason": null,
                "usage": { "input_tokens": 50, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0, "output_tokens": 1 }
            }
        }),
        json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": { "type": "tool_use", "id": "toolu_01abc", "name": "get_weather", "input": {} }
        }),
        json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "input_json_delta", "partial_json": "{\"city\":" } }),
        json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "input_json_delta", "partial_json": "\"London\"}" } }),
        json!({ "type": "content_block_stop", "index": 0 }),
        json!({
            "type": "message_delta",
            "delta": { "stop_reason": "tool_use", "stop_sequence": null },
            "usage": { "output_tokens": 15 }
        }),
        json!({ "type": "message_stop" }),
    ];

    let output = run_anthropic_events(events, &model).await;
    assert_eq!(output.stop_reason, StopReason::ToolUse);
    assert!(output.content.iter().any(|b| matches!(
        b,
        ContentBlock::ToolCall { id, name, arguments, .. }
            if id == "toolu_01abc"
            && name == "get_weather"
            && arguments.get("city").and_then(|v| v.as_str()) == Some("London")
    )));
}

#[tokio::test]
async fn inv4_thinking_then_text_streaming() {
    let model = reasoning_model();
    let events = vec![
        json!({
            "type": "message_start",
            "message": {
                "id": "msg_03",
                "usage": { "input_tokens": 30, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0, "output_tokens": 1 }
            }
        }),
        // Thinking block first
        json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "thinking", "thinking": "" } }),
        json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "thinking_delta", "thinking": "Let me think..." } }),
        json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "signature_delta", "signature": "sig_xyz" } }),
        json!({ "type": "content_block_stop", "index": 0 }),
        // Then text block
        json!({ "type": "content_block_start", "index": 1, "content_block": { "type": "text", "text": "" } }),
        json!({ "type": "content_block_delta", "index": 1, "delta": { "type": "text_delta", "text": "The answer is 42." } }),
        json!({ "type": "content_block_stop", "index": 1 }),
        json!({
            "type": "message_delta",
            "delta": { "stop_reason": "end_turn", "stop_sequence": null },
            "usage": { "output_tokens": 25 }
        }),
        json!({ "type": "message_stop" }),
    ];

    let output = run_anthropic_events(events, &model).await;
    assert_eq!(output.content.len(), 2);
    assert!(matches!(output.content[0], ContentBlock::Thinking { .. }));
    assert!(matches!(output.content[1], ContentBlock::Text { .. }));

    // INV-4: thinking block must have signature set
    assert!(matches!(
        &output.content[0],
        ContentBlock::Thinking { thinking, thinking_signature: Some(sig), .. }
            if thinking.contains("Let me think") && sig == "sig_xyz"
    ));

    // INV-4: usage populated
    assert_eq!(output.usage.output, 25);
    assert_eq!(output.usage.input, 30);
}

#[tokio::test]
async fn inv4_cache_tokens_in_usage() {
    let model = anthropic_model();
    let events = vec![
        json!({
            "type": "message_start",
            "message": {
                "id": "msg_cache",
                "usage": {
                    "input_tokens": 100,
                    "cache_read_input_tokens": 500,
                    "cache_creation_input_tokens": 50,
                    "output_tokens": 1
                }
            }
        }),
        json!({
            "type": "message_delta",
            "delta": { "stop_reason": "end_turn" },
            "usage": { "output_tokens": 20 }
        }),
        json!({ "type": "message_stop" }),
    ];

    let output = run_anthropic_events(events, &model).await;
    assert_eq!(output.usage.input, 100);
    assert_eq!(output.usage.cache_read, 500);
    assert_eq!(output.usage.cache_write, 50);
    assert_eq!(output.usage.output, 20);
    assert_eq!(output.usage.total_tokens, 100 + 500 + 50 + 20);
}

#[tokio::test]
async fn inv4_unknown_event_type_skipped_no_panic() {
    let model = anthropic_model();
    let events = vec![
        json!({ "type": "some_future_event_type", "data": "ignored" }),
        json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "text", "text": "" } }),
        json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": "ok" } }),
        json!({ "type": "content_block_stop", "index": 0 }),
        json!({
            "type": "message_delta",
            "delta": { "stop_reason": "end_turn" },
            "usage": { "output_tokens": 1 }
        }),
        json!({ "type": "message_stop" }),
    ];

    let output = run_anthropic_events(events, &model).await;
    assert_eq!(output.stop_reason, StopReason::Stop);
    assert!(output
        .content
        .iter()
        .any(|b| matches!(b, ContentBlock::Text { text, .. } if text == "ok")));
}

#[tokio::test]
async fn inv4_error_event_returns_error() {
    let model = anthropic_model();
    let events = vec![json!({
        "type": "error",
        "error": { "type": "overloaded_error", "message": "Overloaded" }
    })];

    let mut output = AssistantMessage {
        role: "assistant".into(),
        content: Vec::new(),
        api: model.api.clone(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        usage: Usage::default(),
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 0,
    };

    let (mut tx, _stream) = assistant_message_event_stream();
    let result = process_anthropic_events(events, &mut output, &mut tx, &model).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Overloaded"));
}

// =============================================================================
// INV-5: Cost calculation matches expected values for known model pricing
// =============================================================================

#[tokio::test]
async fn inv5_cost_calculated_from_usage() {
    // claude-3-5-haiku: input=$0.8/M, output=$4.0/M, cache_read=$0.08/M, cache_write=$1.0/M
    let model = anthropic_model();
    let events = vec![
        json!({
            "type": "message_start",
            "message": {
                "id": "msg_cost",
                "usage": {
                    "input_tokens": 1_000_000,
                    "cache_read_input_tokens": 0,
                    "cache_creation_input_tokens": 0,
                    "output_tokens": 1
                }
            }
        }),
        json!({
            "type": "message_delta",
            "delta": { "stop_reason": "end_turn" },
            "usage": { "output_tokens": 1_000_000 }
        }),
        json!({ "type": "message_stop" }),
    ];

    let output = run_anthropic_events(events, &model).await;
    // 1M input @ $0.8/M = $0.80
    assert!(
        (output.usage.cost.input - 0.8).abs() < 1e-9,
        "input cost: {}",
        output.usage.cost.input
    );
    // 1M output @ $4.0/M = $4.00
    assert!(
        (output.usage.cost.output - 4.0).abs() < 1e-9,
        "output cost: {}",
        output.usage.cost.output
    );
    assert!(
        (output.usage.cost.total - 4.8).abs() < 1e-9,
        "total: {}",
        output.usage.cost.total
    );
}

#[tokio::test]
async fn inv5_cache_costs_calculated_separately() {
    // cache_read=$0.08/M, cache_write=$1.0/M
    let model = anthropic_model();
    let events = vec![
        json!({
            "type": "message_start",
            "message": {
                "id": "msg_cache_cost",
                "usage": {
                    "input_tokens": 0,
                    "cache_read_input_tokens": 1_000_000,
                    "cache_creation_input_tokens": 1_000_000,
                    "output_tokens": 1
                }
            }
        }),
        json!({
            "type": "message_delta",
            "delta": { "stop_reason": "end_turn" },
            "usage": { "output_tokens": 0 }
        }),
        json!({ "type": "message_stop" }),
    ];

    let output = run_anthropic_events(events, &model).await;
    // 1M cache_read @ $0.08/M = $0.08
    assert!(
        (output.usage.cost.cache_read - 0.08).abs() < 1e-9,
        "cache_read: {}",
        output.usage.cost.cache_read
    );
    // 1M cache_write @ $1.0/M = $1.00
    assert!(
        (output.usage.cost.cache_write - 1.0).abs() < 1e-9,
        "cache_write: {}",
        output.usage.cost.cache_write
    );
    assert!(
        (output.usage.cost.total - 1.08).abs() < 1e-9,
        "total: {}",
        output.usage.cost.total
    );
}

// =============================================================================
// Tool definitions conversion
// =============================================================================

#[test]
fn tools_convert_to_anthropic_format() {
    let tools = vec![Tool {
        name: "get_weather".into(),
        description: "Get current weather".into(),
        parameters: json!({
            "type": "object",
            "properties": { "location": { "type": "string" } },
            "required": ["location"]
        }),
    }];
    let result = convert_anthropic_tools(&tools);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0]["name"], "get_weather");
    assert_eq!(result[0]["description"], "Get current weather");
    assert!(result[0]["input_schema"].is_object());
    assert_eq!(result[0]["input_schema"]["type"], "object");
    // Should NOT have "parameters" key (that's OpenAI format)
    assert!(result[0].get("parameters").is_none());
}

// =============================================================================
// Request body construction
// =============================================================================

#[test]
fn request_body_includes_model_and_stream() {
    let model = anthropic_model();
    let context = Context {
        system_prompt: Some("Be helpful.".into()),
        messages: vec![],
        tools: None,
    };
    let body = build_request_body(&model, &context, &AnthropicRequestOptions::default());
    assert_eq!(body["model"], "claude-3-5-haiku-20241022");
    assert_eq!(body["stream"], true);
    assert!(body["max_tokens"].as_u64().unwrap() > 0);
    // System prompt is now an array of content blocks with cache_control
    let sys = &body["system"];
    assert!(sys.is_array());
    assert_eq!(sys[0]["type"], "text");
    assert_eq!(sys[0]["text"], "Be helpful.");
    assert_eq!(sys[0]["cache_control"]["type"], "ephemeral");
}

#[test]
fn request_body_includes_tools_when_present() {
    let model = anthropic_model();
    let context = Context {
        system_prompt: None,
        messages: vec![],
        tools: Some(vec![Tool {
            name: "search".into(),
            description: "Search the web".into(),
            parameters: json!({ "type": "object" }),
        }]),
    };
    let body = build_request_body(&model, &context, &AnthropicRequestOptions::default());
    assert!(body["tools"].is_array());
    assert_eq!(body["tools"][0]["name"], "search");
    assert!(body["tools"][0]["input_schema"].is_object());
    // Last tool gets cache_control for prompt caching
    assert_eq!(body["tools"][0]["cache_control"]["type"], "ephemeral");
}

#[test]
fn request_body_no_tools_key_when_empty() {
    let model = anthropic_model();
    let context = Context {
        system_prompt: None,
        messages: vec![],
        tools: None,
    };
    let body = build_request_body(&model, &context, &AnthropicRequestOptions::default());
    assert!(body.get("tools").is_none() || body["tools"].is_null());
}

#[test]
fn request_body_thinking_config_for_reasoning_model() {
    let model = non_xhigh_reasoning_model(); // non-xhigh → budget-based
    let context = Context {
        system_prompt: None,
        messages: vec![],
        tools: None,
    };
    let opts = AnthropicRequestOptions {
        thinking_config: Some(json!({ "type": "enabled", "budget_tokens": 5000 })),
        ..Default::default()
    };
    let body = build_request_body(&model, &context, &opts);
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["thinking"]["budget_tokens"], 5000);
}

#[test]
fn request_body_adaptive_thinking_for_opus_46() {
    let model = reasoning_model(); // claude-opus-4-6 → xhigh → adaptive
    let context = Context {
        system_prompt: None,
        messages: vec![],
        tools: None,
    };
    let opts = AnthropicRequestOptions {
        thinking_config: Some(json!({ "type": "adaptive" })),
        ..Default::default()
    };
    let body = build_request_body(&model, &context, &opts);
    assert_eq!(body["thinking"]["type"], "adaptive");
}

// =============================================================================
// Missing API key emits error event
// =============================================================================

#[tokio::test]
async fn missing_api_key_emits_error_event() {
    use ai::providers::anthropic::AnthropicProvider;
    use ai::providers::ApiProvider;
    use futures::StreamExt;

    // Ensure ANTHROPIC_API_KEY is NOT set
    // (we pass an explicit empty key to force the error path)
    let provider = AnthropicProvider::new();
    let model = anthropic_model();
    let context = Context {
        system_prompt: None,
        messages: vec![],
        tools: None,
    };
    let mut opts = ai::types::StreamOptions::default();
    opts.api_key = Some(String::new()); // empty → will fall back to env var

    // Temporarily unset the env var for this test
    let saved = std::env::var("ANTHROPIC_API_KEY").ok();
    std::env::remove_var("ANTHROPIC_API_KEY");

    let mut stream = provider.stream(&model, &context, Some(&opts));
    let mut got_error = false;
    while let Some(event) = stream.next().await {
        if matches!(event, ai::types::AssistantMessageEvent::Error { .. }) {
            got_error = true;
        }
    }

    // Restore env var if it was set
    if let Some(key) = saved {
        std::env::set_var("ANTHROPIC_API_KEY", key);
    }

    assert!(got_error, "Expected an Error event when API key is missing");
}

// =============================================================================
// Integration tests — require ANTHROPIC_API_KEY
// =============================================================================

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn integration_simple_text_completion() {
    let _guard = registry_lock();
    clear_api_providers();
    register_builtin_providers();

    let model = get_model("anthropic", "claude-3-5-haiku-20241022").unwrap();
    let mut opts = SimpleStreamOptions::default();
    opts.base.api_key = env_key("ANTHROPIC_API_KEY");

    let response = complete_simple(
        &model,
        &Context {
            system_prompt: Some("You are a helpful assistant.".into()),
            messages: vec![Message::User(UserMessage {
                role: "user".into(),
                content: UserContent::Text("Say exactly: hello world".into()),
                timestamp: 0,
            })],
            tools: None,
        },
        Some(&opts),
    )
    .await
    .unwrap();

    assert_eq!(response.stop_reason, StopReason::Stop);
    assert!(response.error_message.is_none());
    assert!(response.content.iter().any(
        |b| matches!(b, ContentBlock::Text { text, .. } if text.to_lowercase().contains("hello"))
    ));

    clear_api_providers();
}

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn integration_tool_call_round_trip() {
    let _guard = registry_lock();
    clear_api_providers();
    register_builtin_providers();

    let model = get_model("anthropic", "claude-3-5-haiku-20241022").unwrap();
    let mut opts = SimpleStreamOptions::default();
    opts.base.api_key = env_key("ANTHROPIC_API_KEY");

    let response = complete_simple(
        &model,
        &Context {
            system_prompt: Some(
                "You are a helpful assistant. Use the provided tools when asked.".into(),
            ),
            messages: vec![Message::User(UserMessage {
                role: "user".into(),
                content: UserContent::Text("Use the echo tool to echo 'test123'".into()),
                timestamp: 0,
            })],
            tools: Some(vec![Tool {
                name: "echo".into(),
                description: "Echo a message".into(),
                parameters: json!({
                    "type": "object",
                    "properties": { "message": { "type": "string" } },
                    "required": ["message"]
                }),
            }]),
        },
        Some(&opts),
    )
    .await
    .unwrap();

    assert_eq!(response.stop_reason, StopReason::ToolUse);
    assert!(response
        .content
        .iter()
        .any(|b| matches!(b, ContentBlock::ToolCall { name, .. } if name == "echo")));

    clear_api_providers();
}

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn integration_reasoning_response() {
    let _guard = registry_lock();
    clear_api_providers();
    register_builtin_providers();

    let model = get_model("anthropic", "claude-3-7-sonnet-20250219").unwrap();
    let mut opts = SimpleStreamOptions::default();
    opts.base.api_key = env_key("ANTHROPIC_API_KEY");
    opts.reasoning = Some(ThinkingLevel::Low);

    let response = complete_simple(
        &model,
        &Context {
            system_prompt: None,
            messages: vec![Message::User(UserMessage {
                role: "user".into(),
                content: UserContent::Text("What is 2 + 2? Think step by step.".into()),
                timestamp: 0,
            })],
            tools: None,
        },
        Some(&opts),
    )
    .await
    .unwrap();

    assert_ne!(response.stop_reason, StopReason::Error);
    assert!(response.error_message.is_none());

    clear_api_providers();
}
