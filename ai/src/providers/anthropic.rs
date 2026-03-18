//! Anthropic Messages API provider implementation.
//!
//! Mirrors: packages/ai/src/providers/anthropic-messages.ts

use std::collections::HashMap;

use anyhow::Result;
use futures::StreamExt;
use serde_json::{json, Value};

use crate::models::{calculate_cost, supports_xhigh};
use crate::providers::ApiProvider;
use crate::stream::{assistant_message_event_stream, AssistantMessageEventSender, AssistantMessageEventStream};
use crate::types::{
    AssistantMessage, AssistantMessageEvent, ContentBlock, Context, Cost, Message, Model,
    SimpleStreamOptions, StopReason, StreamOptions, ThinkingBudgets, ThinkingLevel, Tool, Usage,
    UserBlock, UserContent,
};

// =============================================================================
// Provider struct
// =============================================================================

pub struct AnthropicProvider {
    client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    fn resolve_api_key(&self, model: &Model, options_key: Option<&str>) -> Result<String> {
        if let Some(key) = options_key {
            if !key.is_empty() {
                return Ok(key.to_string());
            }
        }
        let env_var = match model.provider.as_str() {
            "kimi-coding" => "KIMI_API_KEY",
            _ => "ANTHROPIC_API_KEY",
        };
        std::env::var(env_var).map_err(|_| {
            anyhow::anyhow!(
                "No API key for provider '{}'. Set {} or pass api_key.",
                model.provider,
                env_var
            )
        })
    }
}

impl Default for AnthropicProvider {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Stop reason mapping
// =============================================================================

/// Map Anthropic stop_reason strings to tau `StopReason`.
pub fn map_stop_reason(reason: Option<&str>) -> StopReason {
    match reason {
        Some("end_turn") => StopReason::Stop,
        Some("max_tokens") => StopReason::Length,
        Some("tool_use") => StopReason::ToolUse,
        // refusal and pause_turn are non-error completions
        Some("refusal") | Some("pause_turn") => StopReason::Stop,
        None => StopReason::Stop,
        _ => StopReason::Stop,
    }
}

// =============================================================================
// Message conversion
// =============================================================================

/// Convert tau `Context` to Anthropic Messages API format.
///
/// Returns `(system_prompt, messages_array)`.
/// Consecutive messages of the same role are merged (Anthropic requires alternating roles).
pub fn convert_anthropic_messages(model: &Model, context: &Context) -> (Option<String>, Vec<Value>) {
    let system_prompt = context.system_prompt.clone();
    let mut messages: Vec<Value> = Vec::new();

    for msg in &context.messages {
        match msg {
            Message::User(u) => {
                let content: Vec<Value> = match &u.content {
                    UserContent::Text(t) => vec![json!({ "type": "text", "text": t })],
                    UserContent::Blocks(blocks) => blocks
                        .iter()
                        .filter_map(|b| match b {
                            UserBlock::Text { text } => Some(json!({ "type": "text", "text": text })),
                            UserBlock::Image { data, mime_type } => {
                                if model.input.iter().any(|i| i == "image") {
                                    Some(json!({
                                        "type": "image",
                                        "source": {
                                            "type": "base64",
                                            "media_type": mime_type,
                                            "data": data,
                                        }
                                    }))
                                } else {
                                    None
                                }
                            }
                        })
                        .collect(),
                };

                if content.is_empty() {
                    continue;
                }

                // Merge into previous user message if roles align
                if let Some(last) = messages.last_mut() {
                    if last["role"] == "user" {
                        if let Some(arr) = last["content"].as_array_mut() {
                            arr.extend(content);
                            continue;
                        }
                    }
                }

                messages.push(json!({ "role": "user", "content": content }));
            }

            Message::Assistant(a) => {
                let mut content: Vec<Value> = Vec::new();

                for block in &a.content {
                    match block {
                        ContentBlock::Text { text, .. } => {
                            if !text.is_empty() {
                                content.push(json!({ "type": "text", "text": text }));
                            }
                        }
                        ContentBlock::Thinking {
                            thinking,
                            thinking_signature: Some(sig),
                            redacted: Some(true),
                            ..
                        } => {
                            // Redacted thinking block — re-send as redacted_thinking
                            let _ = thinking;
                            content.push(json!({ "type": "redacted_thinking", "data": sig }));
                        }
                        ContentBlock::Thinking {
                            thinking,
                            thinking_signature: Some(sig),
                            ..
                        } => {
                            content.push(json!({
                                "type": "thinking",
                                "thinking": thinking,
                                "signature": sig,
                            }));
                        }
                        ContentBlock::Thinking { .. } => {
                            // No signature (cross-provider thinking) — skip
                        }
                        ContentBlock::ToolCall { id, name, arguments, .. } => {
                            content.push(json!({
                                "type": "tool_use",
                                "id": id,
                                "name": name,
                                "input": arguments,
                            }));
                        }
                        ContentBlock::Image { .. } => {
                            // Images in assistant content are not re-sent
                        }
                    }
                }

                if !content.is_empty() {
                    messages.push(json!({ "role": "assistant", "content": content }));
                }
            }

            Message::ToolResult(tr) => {
                // Build individual content blocks for this tool result
                let mut result_content: Vec<Value> = Vec::new();

                for block in &tr.content {
                    match block {
                        UserBlock::Text { text } => {
                            result_content.push(json!({ "type": "text", "text": text }));
                        }
                        UserBlock::Image { data, mime_type } => {
                            if model.input.iter().any(|i| i == "image") {
                                result_content.push(json!({
                                    "type": "image",
                                    "source": {
                                        "type": "base64",
                                        "media_type": mime_type,
                                        "data": data,
                                    }
                                }));
                            }
                        }
                    }
                }

                let mut tool_result = json!({
                    "type": "tool_result",
                    "tool_use_id": tr.tool_call_id,
                    "content": result_content,
                });
                if tr.is_error {
                    tool_result["is_error"] = json!(true);
                }

                // Merge into previous user message, or create a new one
                if let Some(last) = messages.last_mut() {
                    if last["role"] == "user" {
                        if let Some(arr) = last["content"].as_array_mut() {
                            arr.push(tool_result);
                            continue;
                        }
                    }
                }

                messages.push(json!({ "role": "user", "content": [tool_result] }));
            }
        }
    }

    (system_prompt, messages)
}

/// Convert tau `Tool` definitions to Anthropic `{ name, description, input_schema }` format.
pub fn convert_anthropic_tools(tools: &[Tool]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.parameters,
            })
        })
        .collect()
}

// =============================================================================
// Request body builder
// =============================================================================

/// Options for building the Anthropic Messages request body.
#[derive(Debug, Default, Clone)]
pub struct AnthropicRequestOptions {
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub thinking_config: Option<Value>,
    pub extra_headers: Option<HashMap<String, String>>,
}

/// Build the request body JSON for the Anthropic Messages API.
pub fn build_request_body(model: &Model, context: &Context, opts: &AnthropicRequestOptions) -> Value {
    let (system_prompt, messages) = convert_anthropic_messages(model, context);

    let max_tokens = opts
        .max_tokens
        .unwrap_or_else(|| model.max_tokens.min(u32::MAX as u64) as u32);

    let mut body = json!({
        "model": model.id,
        "max_tokens": max_tokens,
        "messages": messages,
        "stream": true,
    });

    if let Some(sys) = system_prompt {
        body["system"] = json!(sys);
    }

    if let Some(temp) = opts.temperature {
        body["temperature"] = json!(temp);
    }

    if let Some(thinking) = &opts.thinking_config {
        body["thinking"] = thinking.clone();
    }

    if let Some(tools) = &context.tools {
        if !tools.is_empty() {
            body["tools"] = json!(convert_anthropic_tools(tools));
        }
    }

    body
}

// =============================================================================
// Streaming event processing state machine
// =============================================================================

/// Internal state for an in-progress content block.
#[derive(Debug)]
enum BlockState {
    Text {
        text: String,
        content_index: usize,
    },
    Thinking {
        thinking: String,
        signature: String,
        content_index: usize,
    },
    ToolUse {
        id: String,
        name: String,
        json: String,
        content_index: usize,
    },
}

/// Process Anthropic streaming events into `output` and push `AssistantMessageEvent`s via `tx`.
///
/// Public so integration tests can drive it directly with fixture events.
pub async fn process_anthropic_events(
    events: Vec<Value>,
    output: &mut AssistantMessage,
    tx: &mut AssistantMessageEventSender,
    model: &Model,
) -> Result<()> {
    // Anthropic block index → BlockState
    let mut blocks: HashMap<usize, BlockState> = HashMap::new();
    let mut input_tokens: u64 = 0;
    let mut cache_read_tokens: u64 = 0;
    let mut cache_write_tokens: u64 = 0;

    for event in events {
        let event_type = match event.get("type").and_then(|t| t.as_str()) {
            Some(t) => t.to_string(),
            None => continue,
        };

        match event_type.as_str() {
            "message_start" => {
                let usage = &event["message"]["usage"];
                input_tokens = usage["input_tokens"].as_u64().unwrap_or(0);
                cache_read_tokens = usage["cache_read_input_tokens"].as_u64().unwrap_or(0);
                cache_write_tokens = usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
            }

            "content_block_start" => {
                let index = event["index"].as_u64().unwrap_or(0) as usize;
                let block = &event["content_block"];

                match block["type"].as_str() {
                    Some("text") => {
                        let content_index = output.content.len();
                        output.content.push(ContentBlock::Text {
                            text: String::new(),
                            text_signature: None,
                        });
                        blocks.insert(
                            index,
                            BlockState::Text {
                                text: String::new(),
                                content_index,
                            },
                        );
                        tx.push(AssistantMessageEvent::TextStart {
                            content_index,
                            partial: output.clone(),
                        });
                    }

                    Some("thinking") => {
                        let content_index = output.content.len();
                        output.content.push(ContentBlock::Thinking {
                            thinking: String::new(),
                            thinking_signature: None,
                            redacted: None,
                        });
                        blocks.insert(
                            index,
                            BlockState::Thinking {
                                thinking: String::new(),
                                signature: String::new(),
                                content_index,
                            },
                        );
                        tx.push(AssistantMessageEvent::ThinkingStart {
                            content_index,
                            partial: output.clone(),
                        });
                    }

                    Some("tool_use") => {
                        let id = block["id"].as_str().unwrap_or("").to_string();
                        let name = block["name"].as_str().unwrap_or("").to_string();
                        let content_index = output.content.len();
                        output.content.push(ContentBlock::ToolCall {
                            id: id.clone(),
                            name: name.clone(),
                            arguments: HashMap::new(),
                            thought_signature: None,
                        });
                        blocks.insert(
                            index,
                            BlockState::ToolUse {
                                id,
                                name,
                                json: String::new(),
                                content_index,
                            },
                        );
                        tx.push(AssistantMessageEvent::ToolCallStart {
                            content_index,
                            partial: output.clone(),
                        });
                    }

                    _ => {}
                }
            }

            "content_block_delta" => {
                let index = event["index"].as_u64().unwrap_or(0) as usize;
                let delta = &event["delta"];

                // Extract the content_index early to avoid borrow conflicts
                let content_index_opt = blocks.get(&index).map(|s| match s {
                    BlockState::Text { content_index, .. } => *content_index,
                    BlockState::Thinking { content_index, .. } => *content_index,
                    BlockState::ToolUse { content_index, .. } => *content_index,
                });

                if let Some(state) = blocks.get_mut(&index) {
                    match state {
                        BlockState::Text { text, content_index } => {
                            if delta["type"] == "text_delta" {
                                let d = delta["text"].as_str().unwrap_or("").to_string();
                                text.push_str(&d);
                                let new_text = text.clone();
                                let ci = *content_index;
                                if let Some(ContentBlock::Text { text: bt, .. }) =
                                    output.content.get_mut(ci)
                                {
                                    *bt = new_text;
                                }
                                tx.push(AssistantMessageEvent::TextDelta {
                                    content_index: ci,
                                    delta: d,
                                    partial: output.clone(),
                                });
                            }
                        }

                        BlockState::Thinking {
                            thinking,
                            signature,
                            content_index,
                        } => {
                            let ci = *content_index;
                            match delta["type"].as_str() {
                                Some("thinking_delta") => {
                                    let d = delta["thinking"].as_str().unwrap_or("").to_string();
                                    thinking.push_str(&d);
                                    let new_thinking = thinking.clone();
                                    if let Some(ContentBlock::Thinking {
                                        thinking: bt, ..
                                    }) = output.content.get_mut(ci)
                                    {
                                        *bt = new_thinking;
                                    }
                                    tx.push(AssistantMessageEvent::ThinkingDelta {
                                        content_index: ci,
                                        delta: d,
                                        partial: output.clone(),
                                    });
                                }
                                Some("signature_delta") => {
                                    let d =
                                        delta["signature"].as_str().unwrap_or("").to_string();
                                    signature.push_str(&d);
                                }
                                _ => {}
                            }
                        }

                        BlockState::ToolUse { json, content_index, .. } => {
                            if delta["type"] == "input_json_delta" {
                                let d =
                                    delta["partial_json"].as_str().unwrap_or("").to_string();
                                json.push_str(&d);
                                let ci = *content_index;
                                tx.push(AssistantMessageEvent::ToolCallDelta {
                                    content_index: ci,
                                    delta: d,
                                    partial: output.clone(),
                                });
                            }
                        }
                    }
                }

                let _ = content_index_opt; // suppress unused warning
            }

            "content_block_stop" => {
                let index = event["index"].as_u64().unwrap_or(0) as usize;

                if let Some(state) = blocks.remove(&index) {
                    match state {
                        BlockState::Text { text, content_index } => {
                            if let Some(ContentBlock::Text { text: bt, .. }) =
                                output.content.get_mut(content_index)
                            {
                                *bt = text.clone();
                            }
                            tx.push(AssistantMessageEvent::TextEnd {
                                content_index,
                                content: text,
                                partial: output.clone(),
                            });
                        }

                        BlockState::Thinking {
                            thinking,
                            signature,
                            content_index,
                        } => {
                            let sig_opt = if signature.is_empty() {
                                None
                            } else {
                                Some(signature)
                            };
                            if let Some(ContentBlock::Thinking {
                                thinking: bt,
                                thinking_signature,
                                ..
                            }) = output.content.get_mut(content_index)
                            {
                                *bt = thinking.clone();
                                *thinking_signature = sig_opt;
                            }
                            tx.push(AssistantMessageEvent::ThinkingEnd {
                                content_index,
                                content: thinking,
                                partial: output.clone(),
                            });
                        }

                        BlockState::ToolUse { id, name, json, content_index } => {
                            let arguments: HashMap<String, Value> =
                                serde_json::from_str(&json).unwrap_or_default();
                            let tool_call = ContentBlock::ToolCall {
                                id,
                                name,
                                arguments,
                                thought_signature: None,
                            };
                            if let Some(block) = output.content.get_mut(content_index) {
                                *block = tool_call.clone();
                            }
                            tx.push(AssistantMessageEvent::ToolCallEnd {
                                content_index,
                                tool_call,
                                partial: output.clone(),
                            });
                        }
                    }
                }
            }

            "message_delta" => {
                let delta = &event["delta"];
                let stop_reason = delta["stop_reason"].as_str();
                output.stop_reason = map_stop_reason(stop_reason);

                let output_tokens = event["usage"]["output_tokens"].as_u64().unwrap_or(0);
                output.usage = Usage {
                    input: input_tokens,
                    output: output_tokens,
                    cache_read: cache_read_tokens,
                    cache_write: cache_write_tokens,
                    total_tokens: input_tokens
                        + cache_read_tokens
                        + cache_write_tokens
                        + output_tokens,
                    cost: Cost::default(),
                };
                calculate_cost(model, &mut output.usage);
            }

            "error" => {
                let error_type = event["error"]["type"].as_str().unwrap_or("unknown");
                let error_msg = event["error"]["message"].as_str().unwrap_or("unknown error");
                return Err(anyhow::anyhow!("{}: {}", error_type, error_msg));
            }

            // message_start (already handled), message_stop, ping, and others are silently ignored
            _ => {}
        }
    }

    Ok(())
}

// =============================================================================
// SSE collection (Anthropic terminates with message_stop, no [DONE])
// =============================================================================

async fn collect_anthropic_sse_events(response: reqwest::Response) -> Result<Vec<Value>> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut events: Vec<Value> = Vec::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        loop {
            match buffer.find('\n') {
                None => break,
                Some(pos) => {
                    let line = buffer[..pos].trim_end_matches('\r').to_string();
                    buffer = buffer[pos + 1..].to_string();

                    if let Some(data) = line.strip_prefix("data: ") {
                        match serde_json::from_str::<Value>(data) {
                            Ok(v) => {
                                let is_stop = v
                                    .get("type")
                                    .and_then(|t| t.as_str())
                                    == Some("message_stop");
                                events.push(v);
                                if is_stop {
                                    return Ok(events);
                                }
                            }
                            Err(_) => {
                                // Malformed JSON line — skip gracefully
                            }
                        }
                    }
                    // event:, ping, and empty lines are ignored
                }
            }
        }
    }

    if events.is_empty() {
        return Err(anyhow::anyhow!("Empty response stream"));
    }

    Ok(events)
}

// =============================================================================
// Core streaming function
// =============================================================================

fn stream_anthropic_messages(
    client: reqwest::Client,
    model: Model,
    context: Context,
    api_key: String,
    opts: AnthropicRequestOptions,
) -> AssistantMessageEventStream {
    let (mut tx, stream) = assistant_message_event_stream();

    tokio::spawn(async move {
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
            timestamp: chrono::Utc::now().timestamp_millis(),
        };

        let body = build_request_body(&model, &context, &opts);
        let url = format!("{}/v1/messages", model.base_url);

        let mut req = client
            .post(&url)
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json");

        if let Some(headers) = &model.headers {
            for (k, v) in headers {
                req = req.header(k, v);
            }
        }
        if let Some(headers) = &opts.extra_headers {
            for (k, v) in headers {
                req = req.header(k, v);
            }
        }

        let response = match req.json(&body).send().await {
            Ok(r) => r,
            Err(e) => {
                output.stop_reason = StopReason::Error;
                output.error_message = Some(e.to_string());
                tx.push(AssistantMessageEvent::Error {
                    reason: StopReason::Error,
                    error: output,
                });
                return;
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            output.stop_reason = StopReason::Error;
            output.error_message = Some(format!("HTTP {}: {}", status, body_text));
            tx.push(AssistantMessageEvent::Error {
                reason: StopReason::Error,
                error: output,
            });
            return;
        }

        tx.push(AssistantMessageEvent::Start {
            partial: output.clone(),
        });

        let events = match collect_anthropic_sse_events(response).await {
            Ok(events) => events,
            Err(e) => {
                output.stop_reason = StopReason::Error;
                output.error_message = Some(e.to_string());
                tx.push(AssistantMessageEvent::Error {
                    reason: StopReason::Error,
                    error: output,
                });
                return;
            }
        };

        if let Err(e) =
            process_anthropic_events(events, &mut output, &mut tx, &model).await
        {
            output.stop_reason = StopReason::Error;
            output.error_message = Some(e.to_string());
            tx.push(AssistantMessageEvent::Error {
                reason: StopReason::Error,
                error: output,
            });
            return;
        }

        if output.stop_reason == StopReason::Error {
            tx.push(AssistantMessageEvent::Error {
                reason: StopReason::Error,
                error: output,
            });
        } else {
            tx.push(AssistantMessageEvent::Done {
                reason: output.stop_reason.clone(),
                message: output,
            });
        }
    });

    stream
}

// =============================================================================
// Thinking configuration helpers
// =============================================================================

/// Build the `thinking` config for the Anthropic request body.
///
/// Opus 4.6+ → `{ type: "adaptive" }`.
/// All other reasoning models → `{ type: "enabled", budget_tokens: N }`.
fn thinking_config_for_level(
    level: &ThinkingLevel,
    budgets: Option<&ThinkingBudgets>,
    model: &Model,
) -> Value {
    if supports_xhigh(model) {
        return json!({ "type": "adaptive" });
    }

    let budget = match level {
        ThinkingLevel::Minimal => budgets.and_then(|b| b.minimal).unwrap_or(1024),
        ThinkingLevel::Low => budgets.and_then(|b| b.low).unwrap_or(2048),
        ThinkingLevel::Medium => budgets.and_then(|b| b.medium).unwrap_or(5000),
        ThinkingLevel::High => budgets.and_then(|b| b.high).unwrap_or(10_000),
        ThinkingLevel::XHigh => budgets.and_then(|b| b.high).unwrap_or(16_000),
    };

    json!({ "type": "enabled", "budget_tokens": budget })
}

// =============================================================================
// ApiProvider implementation
// =============================================================================

impl ApiProvider for AnthropicProvider {
    fn api(&self) -> &str {
        crate::types::known_api::ANTHROPIC_MESSAGES
    }

    fn stream(
        &self,
        model: &Model,
        context: &Context,
        options: Option<&StreamOptions>,
    ) -> AssistantMessageEventStream {
        let default_opts = StreamOptions::default();
        let base = options.unwrap_or(&default_opts);

        let api_key = match self.resolve_api_key(model, base.api_key.as_deref()) {
            Ok(k) => k,
            Err(e) => {
                let (mut tx, stream) = assistant_message_event_stream();
                let err_msg = AssistantMessage {
                    role: "assistant".into(),
                    content: Vec::new(),
                    api: model.api.clone(),
                    provider: model.provider.clone(),
                    model: model.id.clone(),
                    usage: Usage::default(),
                    stop_reason: StopReason::Error,
                    error_message: Some(e.to_string()),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                };
                tx.push(AssistantMessageEvent::Error {
                    reason: StopReason::Error,
                    error: err_msg,
                });
                return stream;
            }
        };

        let opts = AnthropicRequestOptions {
            temperature: base.temperature,
            max_tokens: base.max_tokens,
            thinking_config: None,
            extra_headers: base.headers.clone(),
        };

        stream_anthropic_messages(
            self.client.clone(),
            model.clone(),
            context.clone(),
            api_key,
            opts,
        )
    }

    fn stream_simple(
        &self,
        model: &Model,
        context: &Context,
        options: Option<&SimpleStreamOptions>,
    ) -> AssistantMessageEventStream {
        let default_simple = SimpleStreamOptions::default();
        let opts = options.unwrap_or(&default_simple);
        let base = &opts.base;

        let api_key = match self.resolve_api_key(model, base.api_key.as_deref()) {
            Ok(k) => k,
            Err(e) => {
                let (mut tx, stream) = assistant_message_event_stream();
                let err_msg = AssistantMessage {
                    role: "assistant".into(),
                    content: Vec::new(),
                    api: model.api.clone(),
                    provider: model.provider.clone(),
                    model: model.id.clone(),
                    usage: Usage::default(),
                    stop_reason: StopReason::Error,
                    error_message: Some(e.to_string()),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                };
                tx.push(AssistantMessageEvent::Error {
                    reason: StopReason::Error,
                    error: err_msg,
                });
                return stream;
            }
        };

        let thinking_config = opts
            .reasoning
            .as_ref()
            .map(|level| thinking_config_for_level(level, opts.thinking_budgets.as_ref(), model));

        let request_opts = AnthropicRequestOptions {
            temperature: base.temperature,
            max_tokens: base.max_tokens,
            thinking_config,
            extra_headers: base.headers.clone(),
        };

        stream_anthropic_messages(
            self.client.clone(),
            model.clone(),
            context.clone(),
            api_key,
            request_opts,
        )
    }
}
