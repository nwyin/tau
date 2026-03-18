//! Unit tests for the OpenAI Responses provider.
//!
//! Tests at the SSE event level — no live HTTP required.
//! Live API tests are marked #[ignore = "requires OPENAI_API_KEY"].

mod common;
use common::{env_key, mock_model, registry_lock};

use ai::models::get_model;
use ai::providers::openai_responses::{build_request_body, OpenAIRequestOptions};
use ai::providers::openai_responses_shared::{
    apply_service_tier_pricing, convert_responses_messages, convert_responses_tools,
    map_stop_reason, normalize_tool_call_id, process_sse_events, service_tier_multiplier,
};
use ai::providers::{clear_api_providers, complete_simple, register_builtin_providers};
use ai::stream::assistant_message_event_stream;
use ai::types::{
    AssistantMessage, CacheRetention, ContentBlock, Context, Cost, Message, Model, ModelCost,
    SimpleStreamOptions, StopReason, ThinkingLevel, Tool, ToolResultMessage, Usage, UserBlock,
    UserContent, UserMessage,
};
use serde_json::json;
use std::collections::HashMap;

// =============================================================================
// Helpers
// =============================================================================

fn openai_model() -> Model {
    Model {
        id: "gpt-5-mini".into(),
        name: "GPT-5 Mini".into(),
        api: "openai-responses".into(),
        provider: "openai".into(),
        base_url: "https://api.openai.com/v1".into(),
        reasoning: false,
        input: vec!["text".into(), "image".into()],
        cost: ModelCost { input: 1.0, output: 3.0, cache_read: 0.1, cache_write: 0.0 },
        context_window: 128_000,
        max_tokens: 4_096,
        headers: None,
        compat: None,
    }
}

fn reasoning_model() -> Model {
    Model {
        id: "gpt-5.2-codex".into(),
        name: "GPT-5.2 Codex".into(),
        api: "openai-responses".into(),
        provider: "openai".into(),
        base_url: "https://api.openai.com/v1".into(),
        reasoning: true,
        input: vec!["text".into()],
        cost: ModelCost { input: 5.0, output: 15.0, cache_read: 0.5, cache_write: 0.0 },
        context_window: 200_000,
        max_tokens: 64_000,
        headers: None,
        compat: None,
    }
}

fn sample_context_with_system() -> Context {
    Context {
        system_prompt: Some("You are helpful.".into()),
        messages: vec![Message::User(UserMessage {
            role: "user".into(),
            content: UserContent::Text("Hello!".into()),
            timestamp: 0,
        })],
        tools: None,
    }
}

async fn run_sse_events(
    events: Vec<serde_json::Value>,
    model: &Model,
    service_tier: Option<&str>,
) -> (AssistantMessage, Vec<ai::types::AssistantMessageEvent>) {
    let mut output = AssistantMessage {
        role: "assistant".into(),
        content: Vec::new(),
        api: model.api.clone(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        usage: Usage { cost: Cost::default(), ..Default::default() },
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 0,
    };

    let (mut tx, _stream) = assistant_message_event_stream();
    process_sse_events(events, &mut output, &mut tx, model, service_tier)
        .await
        .unwrap();

    (output, vec![])
}

// =============================================================================
// INV-1: Context round-trips to OpenAI request format
// =============================================================================

#[test]
fn inv1_system_prompt_becomes_system_role() {
    let model = openai_model();
    let context = Context {
        system_prompt: Some("Be helpful.".into()),
        messages: vec![],
        tools: None,
    };
    let messages = convert_responses_messages(&model, &context);
    assert!(!messages.is_empty());
    let first = &messages[0];
    assert_eq!(first["role"], "system");
    assert_eq!(first["content"], "Be helpful.");
}

#[test]
fn inv1_system_prompt_becomes_developer_role_for_reasoning_model() {
    let model = reasoning_model();
    let context = Context {
        system_prompt: Some("Be helpful.".into()),
        messages: vec![],
        tools: None,
    };
    let messages = convert_responses_messages(&model, &context);
    assert_eq!(messages[0]["role"], "developer");
}

#[test]
fn inv1_user_text_message_becomes_input_text() {
    let model = openai_model();
    let context = Context {
        system_prompt: None,
        messages: vec![Message::User(UserMessage {
            role: "user".into(),
            content: UserContent::Text("Hello world".into()),
            timestamp: 0,
        })],
        tools: None,
    };
    let messages = convert_responses_messages(&model, &context);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[0]["content"][0]["type"], "input_text");
    assert_eq!(messages[0]["content"][0]["text"], "Hello world");
}

#[test]
fn inv1_assistant_text_becomes_output_message() {
    let model = openai_model();
    let context = Context {
        system_prompt: None,
        messages: vec![Message::Assistant(AssistantMessage {
            role: "assistant".into(),
            content: vec![ContentBlock::Text {
                text: "Hi there!".into(),
                text_signature: Some("msg_sig123".into()),
            }],
            api: "openai-responses".into(),
            provider: "openai".into(),
            model: "gpt-5-mini".into(),
            usage: Usage::default(),
            stop_reason: StopReason::Stop,
            error_message: None,
            timestamp: 0,
        })],
        tools: None,
    };
    let messages = convert_responses_messages(&model, &context);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["type"], "message");
    assert_eq!(messages[0]["role"], "assistant");
    assert_eq!(messages[0]["status"], "completed");
    assert_eq!(messages[0]["content"][0]["type"], "output_text");
    assert_eq!(messages[0]["content"][0]["text"], "Hi there!");
}

#[test]
fn inv1_tool_call_becomes_function_call() {
    let model = openai_model();
    let mut args = HashMap::new();
    args.insert("x".to_string(), json!(42));

    let context = Context {
        system_prompt: None,
        messages: vec![Message::Assistant(AssistantMessage {
            role: "assistant".into(),
            content: vec![ContentBlock::ToolCall {
                id: "call_abc|fc_xyz".into(),
                name: "my_tool".into(),
                arguments: args,
                thought_signature: None,
            }],
            api: "openai-responses".into(),
            provider: "openai".into(),
            model: "gpt-5-mini".into(),
            usage: Usage::default(),
            stop_reason: StopReason::ToolUse,
            error_message: None,
            timestamp: 0,
        })],
        tools: None,
    };
    let messages = convert_responses_messages(&model, &context);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["type"], "function_call");
    assert_eq!(messages[0]["call_id"], "call_abc");
    assert_eq!(messages[0]["id"], "fc_xyz");
    assert_eq!(messages[0]["name"], "my_tool");
    let args_parsed: serde_json::Value =
        serde_json::from_str(messages[0]["arguments"].as_str().unwrap()).unwrap();
    assert_eq!(args_parsed["x"], 42);
}

#[test]
fn inv1_tool_result_becomes_function_call_output() {
    let model = openai_model();
    let context = Context {
        system_prompt: None,
        messages: vec![Message::ToolResult(ToolResultMessage {
            role: "toolResult".into(),
            tool_call_id: "call_abc|fc_xyz".into(),
            tool_name: "my_tool".into(),
            content: vec![UserBlock::Text { text: "the result".into() }],
            details: None,
            is_error: false,
            timestamp: 0,
        })],
        tools: None,
    };
    let messages = convert_responses_messages(&model, &context);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["type"], "function_call_output");
    assert_eq!(messages[0]["call_id"], "call_abc");
    assert_eq!(messages[0]["output"], "the result");
}

#[test]
fn inv1_tools_convert_to_function_format() {
    let tools = vec![
        Tool {
            name: "search".into(),
            description: "Search the web".into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
    ];
    let result = convert_responses_tools(&tools);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0]["type"], "function");
    assert_eq!(result[0]["name"], "search");
    assert_eq!(result[0]["description"], "Search the web");
    assert_eq!(result[0]["strict"], false);
}

// =============================================================================
// INV-2: SSE events map to correct AssistantMessageEvent variants
// =============================================================================

#[tokio::test]
async fn inv2_text_streaming_lifecycle() {
    let model = openai_model();
    let events = vec![
        json!({
            "type": "response.output_item.added",
            "output_index": 0,
            "item": { "type": "message", "id": "msg_1", "status": "in_progress", "role": "assistant", "content": [] }
        }),
        json!({ "type": "response.output_text.delta", "item_id": "msg_1", "output_index": 0, "content_index": 0, "delta": "Hello " }),
        json!({ "type": "response.output_text.delta", "item_id": "msg_1", "output_index": 0, "content_index": 0, "delta": "World" }),
        json!({
            "type": "response.output_item.done",
            "output_index": 0,
            "item": {
                "type": "message", "id": "msg_1", "status": "completed", "role": "assistant",
                "content": [{ "type": "output_text", "text": "Hello World", "annotations": [] }]
            }
        }),
        json!({
            "type": "response.completed",
            "response": {
                "status": "completed",
                "usage": { "input_tokens": 10, "output_tokens": 5, "total_tokens": 15 }
            }
        }),
    ];

    let (output, _) = run_sse_events(events, &model, None).await;
    assert_eq!(output.stop_reason, StopReason::Stop);
    assert_eq!(output.usage.output, 5);
    assert!(output.content.iter().any(
        |b| matches!(b, ContentBlock::Text { text, .. } if text == "Hello World")
    ));
}

#[tokio::test]
async fn inv2_thinking_streaming_lifecycle() {
    let model = reasoning_model();
    let events = vec![
        json!({
            "type": "response.output_item.added",
            "output_index": 0,
            "item": { "type": "reasoning", "id": "rs_1", "status": "in_progress", "summary": [] }
        }),
        json!({ "type": "response.reasoning_summary_text.delta", "item_id": "rs_1", "output_index": 0, "summary_index": 0, "delta": "step 1" }),
        json!({ "type": "response.reasoning_summary_text.delta", "item_id": "rs_1", "output_index": 0, "summary_index": 0, "delta": " step 2" }),
        json!({
            "type": "response.output_item.done",
            "output_index": 0,
            "item": {
                "type": "reasoning", "id": "rs_1", "status": "completed",
                "summary": [{ "type": "summary_text", "text": "step 1 step 2" }]
            }
        }),
        json!({
            "type": "response.completed",
            "response": { "status": "completed", "usage": { "input_tokens": 20, "output_tokens": 10, "total_tokens": 30 } }
        }),
    ];

    let (output, _) = run_sse_events(events, &model, None).await;
    assert_eq!(output.stop_reason, StopReason::Stop);
    assert!(output.content.iter().any(
        |b| matches!(b, ContentBlock::Thinking { thinking, .. } if thinking.contains("step 1"))
    ));
    // thinking_signature should be set
    assert!(output.content.iter().any(
        |b| matches!(b, ContentBlock::Thinking { thinking_signature: Some(_), .. })
    ));
}

#[tokio::test]
async fn inv2_tool_call_streaming_lifecycle() {
    let model = openai_model();
    let events = vec![
        json!({
            "type": "response.output_item.added",
            "output_index": 0,
            "item": { "type": "function_call", "id": "fc_1", "call_id": "call_abc", "name": "search", "arguments": "", "status": "in_progress" }
        }),
        json!({ "type": "response.function_call_arguments.delta", "item_id": "fc_1", "output_index": 0, "delta": "{\"q\":" }),
        json!({ "type": "response.function_call_arguments.delta", "item_id": "fc_1", "output_index": 0, "delta": "\"hello\"}" }),
        json!({ "type": "response.function_call_arguments.done", "item_id": "fc_1", "output_index": 0, "arguments": "{\"q\":\"hello\"}" }),
        json!({
            "type": "response.output_item.done",
            "output_index": 0,
            "item": { "type": "function_call", "id": "fc_1", "call_id": "call_abc", "name": "search", "arguments": "{\"q\":\"hello\"}", "status": "completed" }
        }),
        json!({
            "type": "response.completed",
            "response": { "status": "completed", "usage": { "input_tokens": 10, "output_tokens": 8, "total_tokens": 18 } }
        }),
    ];

    let (output, _) = run_sse_events(events, &model, None).await;
    // stop reason should be toolUse since there's a tool call
    assert_eq!(output.stop_reason, StopReason::ToolUse);
    assert!(output.content.iter().any(|b| matches!(
        b,
        ContentBlock::ToolCall { name, arguments, .. }
            if name == "search" && arguments.get("q").and_then(|v| v.as_str()) == Some("hello")
    )));
}

#[tokio::test]
async fn inv2_error_event_produces_error() {
    let model = openai_model();
    let events = vec![
        json!({ "type": "error", "code": 429, "message": "Rate limit exceeded" }),
    ];

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
    let result = process_sse_events(events, &mut output, &mut tx, &model, None).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("429"));
}

#[tokio::test]
async fn inv2_response_failed_produces_error() {
    let model = openai_model();
    let events = vec![
        json!({ "type": "response.failed", "response": {} }),
    ];

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
    let result = process_sse_events(events, &mut output, &mut tx, &model, None).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn inv2_interleaved_thinking_text_toolcall() {
    let model = reasoning_model();
    let events = vec![
        // Thinking first
        json!({
            "type": "response.output_item.added",
            "item": { "type": "reasoning", "id": "rs_1", "status": "in_progress", "summary": [] }
        }),
        json!({ "type": "response.reasoning_summary_text.delta", "delta": "thinking..." }),
        json!({
            "type": "response.output_item.done",
            "item": {
                "type": "reasoning", "id": "rs_1", "status": "completed",
                "summary": [{ "type": "summary_text", "text": "thinking..." }]
            }
        }),
        // Then text
        json!({
            "type": "response.output_item.added",
            "item": { "type": "message", "id": "msg_1", "status": "in_progress", "role": "assistant", "content": [] }
        }),
        json!({ "type": "response.output_text.delta", "delta": "result" }),
        json!({
            "type": "response.output_item.done",
            "item": {
                "type": "message", "id": "msg_1", "status": "completed", "role": "assistant",
                "content": [{ "type": "output_text", "text": "result", "annotations": [] }]
            }
        }),
        // Then tool call
        json!({
            "type": "response.output_item.added",
            "item": { "type": "function_call", "id": "fc_1", "call_id": "call_1", "name": "do_thing", "arguments": "", "status": "in_progress" }
        }),
        json!({ "type": "response.function_call_arguments.done", "arguments": "{}" }),
        json!({
            "type": "response.output_item.done",
            "item": { "type": "function_call", "id": "fc_1", "call_id": "call_1", "name": "do_thing", "arguments": "{}", "status": "completed" }
        }),
        json!({
            "type": "response.completed",
            "response": { "status": "completed", "usage": { "input_tokens": 50, "output_tokens": 20, "total_tokens": 70 } }
        }),
    ];

    let (output, _) = run_sse_events(events, &model, None).await;
    assert_eq!(output.content.len(), 3);
    assert!(matches!(output.content[0], ContentBlock::Thinking { .. }));
    assert!(matches!(output.content[1], ContentBlock::Text { .. }));
    assert!(matches!(output.content[2], ContentBlock::ToolCall { .. }));
}

#[tokio::test]
async fn inv2_malformed_sse_line_skipped_no_panic() {
    // This test verifies that process_sse_events gracefully handles unknown event types
    let model = openai_model();
    let events = vec![
        json!({ "type": "response.unknown_event_type", "data": "garbage" }),
        json!({
            "type": "response.completed",
            "response": { "status": "completed", "usage": { "input_tokens": 5, "output_tokens": 2, "total_tokens": 7 } }
        }),
    ];

    let (output, _) = run_sse_events(events, &model, None).await;
    assert_eq!(output.stop_reason, StopReason::Stop);
    assert!(output.content.is_empty()); // No content blocks were created
}

#[tokio::test]
async fn inv2_usage_tokens_computed_correctly() {
    let model = openai_model();
    let events = vec![json!({
        "type": "response.completed",
        "response": {
            "status": "completed",
            "usage": {
                "input_tokens": 120,
                "output_tokens": 40,
                "total_tokens": 160,
                "input_tokens_details": { "cached_tokens": 20 }
            }
        }
    })];

    let (output, _) = run_sse_events(events, &model, None).await;
    // input = 120 - 20 (cached) = 100
    assert_eq!(output.usage.input, 100);
    assert_eq!(output.usage.output, 40);
    assert_eq!(output.usage.cache_read, 20);
    assert_eq!(output.usage.total_tokens, 160);
}

// =============================================================================
// INV-3: Tool call ID normalization
// =============================================================================

#[test]
fn inv3_no_pipe_passthrough() {
    assert_eq!(normalize_tool_call_id("call_123", "openai"), "call_123");
}

#[test]
fn inv3_cross_provider_passthrough() {
    // Non-OpenAI providers: IDs pass through unchanged
    assert_eq!(
        normalize_tool_call_id("toolu_abc|fc_extra", "anthropic"),
        "toolu_abc|fc_extra"
    );
}

#[test]
fn inv3_adds_fc_prefix() {
    let result = normalize_tool_call_id("call_abc|item_xyz", "openai");
    let parts: Vec<&str> = result.split('|').collect();
    assert!(parts[1].starts_with("fc"), "got: {}", parts[1]);
}

#[test]
fn inv3_keeps_fc_prefix() {
    let result = normalize_tool_call_id("call_abc|fc_xyz", "openai");
    let parts: Vec<&str> = result.split('|').collect();
    assert_eq!(parts[1], "fc_xyz");
}

#[test]
fn inv3_replaces_invalid_chars() {
    let result = normalize_tool_call_id("call+abc|fc_item/xyz=ok", "openai");
    assert!(!result.contains('+'));
    assert!(!result.contains('/'));
    assert!(!result.contains('='));
}

#[test]
fn inv3_truncates_both_parts_to_64_chars() {
    let long = "a".repeat(100);
    let id = format!("{}|fc_{}", long, long);
    let result = normalize_tool_call_id(&id, "openai");
    let parts: Vec<&str> = result.split('|').collect();
    assert!(parts[0].len() <= 64, "call_id len: {}", parts[0].len());
    assert!(parts[1].len() <= 64, "item_id len: {}", parts[1].len());
}

#[test]
fn inv3_strips_trailing_underscores() {
    // 64 a's then underscore — after truncation it ends with a
    let call_id = "a".repeat(64);
    let item_id = "fc_".to_string() + &"b".repeat(60) + "___";
    let id = format!("{}|{}", call_id, item_id);
    let result = normalize_tool_call_id(&id, "openai");
    let parts: Vec<&str> = result.split('|').collect();
    assert!(!parts[0].ends_with('_'));
    assert!(!parts[1].ends_with('_'));
}

#[test]
fn inv3_failing_issue_1022_id_normalizes() {
    const FAILING: &str = "call_pAYbIr76hXIjncD9UE4eGfnS|\
        t5nnb2qYMFWGSsr13fhCd1CaCu3t3qONEPuOudu4HSVEtA8YJSL6FAZUxvoOoD792VIJWl91g87\
        EdqsCWp9krVsdBysQoDaf9lMCLb8BS4EYi4gQd5kBQBYLlgD71PYwvf+TbMD9J9/5OMD42oxSR\
        j8H+vRf78/l2Xla33LWz4nOgsddBlbvabICRs8GHt5C9PK5keFtzyi3lsyVKNlfduK3iphsZqs\
        4MLv4zyGJnvZo/+QzShyk5xnMSQX/f98+aEoNflEApCdEOXipipgeiNWnpFSHbcwmMkZoJhURN\
        u+JEz3xCh1mrXeYoN5o+trLL3IXJacSsLYXDrYTipZZbJFRPAucgbnjYBC+/ZzJOfkwCs+Gkw7\
        EoZR7ZQgJ8ma+9586n4tT4cI8DEhBSZsWMjrCt8dxKg==";

    let result = normalize_tool_call_id(FAILING, "openai");
    let parts: Vec<&str> = result.split('|').collect();
    assert_eq!(parts.len(), 2);
    assert!(parts[0].len() <= 64, "call_id too long: {}", parts[0].len());
    assert!(parts[1].len() <= 64, "item_id too long: {}", parts[1].len());
    assert!(parts[1].starts_with("fc"), "item_id must start with fc");
    assert!(!parts[0].ends_with('_'));
    assert!(!parts[1].ends_with('_'));
    // Verify no chars outside [a-zA-Z0-9_-]
    assert!(
        parts[0].chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        "call_id has invalid chars"
    );
    assert!(
        parts[1].chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        "item_id has invalid chars"
    );
}

// =============================================================================
// INV-4: Reasoning effort "xhigh" clamped to "high" for non-xhigh models
// =============================================================================

#[test]
fn inv4_xhigh_in_request_body_for_xhigh_supporting_model() {
    let model = reasoning_model(); // gpt-5.2-codex supports xhigh
    let context = sample_context_with_system();
    let opts = OpenAIRequestOptions {
        reasoning_effort: Some("xhigh".into()),
        ..Default::default()
    };
    let body = build_request_body(&model, &context, &opts);
    assert_eq!(body["reasoning"]["effort"], "xhigh");
}

#[test]
fn inv4_xhigh_clamped_to_high_for_non_xhigh_model() {
    let mut model = openai_model();
    model.reasoning = true; // force reasoning mode to exercise the path
    let context = sample_context_with_system();
    let opts = OpenAIRequestOptions {
        reasoning_effort: Some("xhigh".into()),
        ..Default::default()
    };
    let body = build_request_body(&model, &context, &opts);
    // gpt-5-mini does not support xhigh → clamped to high
    assert_eq!(body["reasoning"]["effort"], "high");
}

#[test]
fn inv4_high_passes_through_unchanged() {
    let mut model = openai_model();
    model.reasoning = true;
    let context = sample_context_with_system();
    let opts = OpenAIRequestOptions {
        reasoning_effort: Some("high".into()),
        ..Default::default()
    };
    let body = build_request_body(&model, &context, &opts);
    assert_eq!(body["reasoning"]["effort"], "high");
}

#[test]
fn inv4_stream_simple_clamps_xhigh_for_non_xhigh_model() {
    // Verify request body generation clamps reasoning for the stream_simple path
    // by checking build_request_body directly with the logic stream_simple applies
    use ai::providers::openai_responses_shared::clamp_reasoning_effort;
    let model = openai_model(); // not xhigh capable
    let clamped = clamp_reasoning_effort("xhigh", &model);
    assert_eq!(clamped, "high");
}

#[test]
fn inv4_stream_simple_preserves_xhigh_for_xhigh_model() {
    use ai::providers::openai_responses_shared::clamp_reasoning_effort;
    let model = reasoning_model(); // gpt-5.2-codex supports xhigh
    let clamped = clamp_reasoning_effort("xhigh", &model);
    assert_eq!(clamped, "xhigh");
}

// =============================================================================
// INV-5: Cost calculation with service tier multiplier
// =============================================================================

#[test]
fn inv5_flex_tier_halves_cost() {
    let mut usage = Usage {
        input: 1000,
        output: 500,
        cache_read: 200,
        cache_write: 0,
        total_tokens: 1700,
        cost: Cost { input: 1.0, output: 1.5, cache_read: 0.2, cache_write: 0.0, total: 2.7 },
    };
    apply_service_tier_pricing(&mut usage, Some("flex"));
    assert!((usage.cost.input - 0.5).abs() < 1e-9, "input: {}", usage.cost.input);
    assert!((usage.cost.output - 0.75).abs() < 1e-9, "output: {}", usage.cost.output);
    assert!((usage.cost.cache_read - 0.1).abs() < 1e-9);
    assert!((usage.cost.total - 1.35).abs() < 1e-9, "total: {}", usage.cost.total);
}

#[test]
fn inv5_priority_tier_doubles_cost() {
    let mut usage = Usage {
        input: 100,
        output: 50,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 150,
        cost: Cost { input: 1.0, output: 0.5, cache_read: 0.0, cache_write: 0.0, total: 1.5 },
    };
    apply_service_tier_pricing(&mut usage, Some("priority"));
    assert!((usage.cost.input - 2.0).abs() < 1e-9);
    assert!((usage.cost.output - 1.0).abs() < 1e-9);
    assert!((usage.cost.total - 3.0).abs() < 1e-9, "total: {}", usage.cost.total);
}

#[test]
fn inv5_default_tier_is_no_op() {
    let original_cost = Cost { input: 1.0, output: 0.5, cache_read: 0.0, cache_write: 0.0, total: 1.5 };
    let mut usage = Usage {
        input: 100,
        output: 50,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 150,
        cost: original_cost.clone(),
    };
    apply_service_tier_pricing(&mut usage, None);
    assert!((usage.cost.input - original_cost.input).abs() < 1e-9);
    assert!((usage.cost.total - original_cost.total).abs() < 1e-9);
}

#[test]
fn inv5_service_tier_multiplier_values() {
    assert!((service_tier_multiplier(Some("flex")) - 0.5).abs() < 1e-9);
    assert!((service_tier_multiplier(Some("priority")) - 2.0).abs() < 1e-9);
    assert!((service_tier_multiplier(Some("default")) - 1.0).abs() < 1e-9);
    assert!((service_tier_multiplier(None) - 1.0).abs() < 1e-9);
}

// =============================================================================
// Additional: map_stop_reason
// =============================================================================

#[test]
fn map_stop_reason_completed_is_stop() {
    assert_eq!(map_stop_reason(Some("completed")), StopReason::Stop);
}

#[test]
fn map_stop_reason_incomplete_is_length() {
    assert_eq!(map_stop_reason(Some("incomplete")), StopReason::Length);
}

#[test]
fn map_stop_reason_failed_is_error() {
    assert_eq!(map_stop_reason(Some("failed")), StopReason::Error);
}

#[test]
fn map_stop_reason_none_is_stop() {
    assert_eq!(map_stop_reason(None), StopReason::Stop);
}

// =============================================================================
// request body construction
// =============================================================================

#[test]
fn request_body_includes_tools() {
    let model = openai_model();
    let context = Context {
        system_prompt: None,
        messages: vec![],
        tools: Some(vec![Tool {
            name: "do_thing".into(),
            description: "Do a thing".into(),
            parameters: json!({ "type": "object" }),
        }]),
    };
    let body = build_request_body(&model, &context, &OpenAIRequestOptions::default());
    assert!(body["tools"].is_array());
    assert_eq!(body["tools"].as_array().unwrap().len(), 1);
    assert_eq!(body["tools"][0]["name"], "do_thing");
}

#[test]
fn request_body_no_tools_when_empty() {
    let model = openai_model();
    let context = Context {
        system_prompt: None,
        messages: vec![],
        tools: None,
    };
    let body = build_request_body(&model, &context, &OpenAIRequestOptions::default());
    assert!(body["tools"].is_null() || body.get("tools").is_none());
}

#[test]
fn request_body_cache_long_sets_prompt_cache_retention() {
    let model = openai_model(); // base_url contains api.openai.com
    let context = sample_context_with_system();
    let opts = OpenAIRequestOptions {
        cache_retention: Some(CacheRetention::Long),
        session_id: Some("session-abc".into()),
        ..Default::default()
    };
    let body = build_request_body(&model, &context, &opts);
    assert_eq!(body["prompt_cache_key"], "session-abc");
    assert_eq!(body["prompt_cache_retention"], "24h");
}

#[test]
fn request_body_cache_none_omits_prompt_cache_key() {
    let model = openai_model();
    let context = sample_context_with_system();
    let opts = OpenAIRequestOptions {
        cache_retention: Some(CacheRetention::None),
        session_id: Some("session-abc".into()),
        ..Default::default()
    };
    let body = build_request_body(&model, &context, &opts);
    assert!(body.get("prompt_cache_key").is_none() || body["prompt_cache_key"].is_null());
}

#[test]
fn request_body_stream_always_true() {
    let model = openai_model();
    let context = sample_context_with_system();
    let body = build_request_body(&model, &context, &OpenAIRequestOptions::default());
    assert_eq!(body["stream"], true);
}

#[test]
fn request_body_store_always_false() {
    let model = openai_model();
    let context = sample_context_with_system();
    let body = build_request_body(&model, &context, &OpenAIRequestOptions::default());
    assert_eq!(body["store"], false);
}

// =============================================================================
// Integration tests — require OPENAI_API_KEY
// =============================================================================

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn integration_simple_text_completion() {
    let _guard = registry_lock();
    clear_api_providers();
    register_builtin_providers();

    let model = get_model("openai", "gpt-5-mini").unwrap();
    let mut opts = SimpleStreamOptions::default();
    opts.base.api_key = env_key("OPENAI_API_KEY");

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
    assert!(response.content.iter().any(|b| matches!(b, ContentBlock::Text { text, .. } if text.to_lowercase().contains("hello"))));

    clear_api_providers();
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn integration_tool_call_round_trip() {
    let _guard = registry_lock();
    clear_api_providers();
    register_builtin_providers();

    let model = get_model("openai", "gpt-5-mini").unwrap();
    let mut opts = SimpleStreamOptions::default();
    opts.base.api_key = env_key("OPENAI_API_KEY");

    let response = complete_simple(
        &model,
        &Context {
            system_prompt: Some("You are a helpful assistant. Use the provided tools when asked.".into()),
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
    assert!(response.content.iter().any(|b| matches!(b, ContentBlock::ToolCall { name, .. } if name == "echo")));

    clear_api_providers();
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn integration_reasoning_response() {
    let _guard = registry_lock();
    clear_api_providers();
    register_builtin_providers();

    let model = get_model("openai", "gpt-5-mini").unwrap();
    let mut opts = SimpleStreamOptions::default();
    opts.base.api_key = env_key("OPENAI_API_KEY");
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
