//! Shared helpers for the OpenAI Chat Completions provider:
//! message conversion, tool format conversion, SSE event processing.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::models::calculate_cost;
use crate::providers::openai_responses_shared::clamp_reasoning_effort;
use crate::stream::AssistantMessageEventSender;
use crate::types::{
    AssistantMessage, AssistantMessageEvent, ContentBlock, Context, Message, Model, StopReason,
    Tool, UserBlock, UserContent,
};

// =============================================================================
// Request options
// =============================================================================

#[derive(Debug, Default, Clone)]
pub struct ChatRequestOptions {
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub reasoning_effort: Option<String>,
    pub extra_headers: Option<HashMap<String, String>>,
}

// =============================================================================
// Message conversion
// =============================================================================

/// Convert tau `Context` to an OpenAI Chat Completions messages array.
pub fn convert_chat_messages(model: &Model, context: &Context) -> Vec<Value> {
    let mut messages: Vec<Value> = Vec::new();

    // System prompt
    if let Some(ref sys) = context.system_prompt {
        messages.push(json!({ "role": "system", "content": sys }));
    }

    for msg in &context.messages {
        match msg {
            Message::User(u) => {
                let content = match &u.content {
                    UserContent::Text(t) => json!(t),
                    UserContent::Blocks(blocks) => {
                        let mut parts = Vec::new();
                        for block in blocks {
                            match block {
                                UserBlock::Text { text } => {
                                    parts.push(json!({ "type": "text", "text": text }));
                                }
                                UserBlock::Image { data, mime_type } => {
                                    if model.input.iter().any(|i| i == "image") {
                                        parts.push(json!({
                                            "type": "image_url",
                                            "image_url": {
                                                "url": format!("data:{};base64,{}", mime_type, data),
                                            }
                                        }));
                                    }
                                }
                            }
                        }
                        if parts.is_empty() {
                            continue;
                        }
                        json!(parts)
                    }
                };

                // Merge consecutive user messages
                if let Some(last) = messages.last() {
                    if last.get("role").and_then(|r| r.as_str()) == Some("user") {
                        // Don't merge — Chat Completions handles consecutive user messages
                        // differently per provider. Just push as separate message.
                    }
                }

                messages.push(json!({ "role": "user", "content": content }));
            }

            Message::Assistant(a) => {
                let mut text_parts: Vec<String> = Vec::new();
                let mut tool_calls: Vec<Value> = Vec::new();

                for block in &a.content {
                    match block {
                        ContentBlock::Text { text, .. } => {
                            if !text.is_empty() {
                                text_parts.push(text.clone());
                            }
                        }
                        ContentBlock::ToolCall {
                            id,
                            name,
                            arguments,
                            ..
                        } => {
                            // Use the id directly — no pipe-separated format for Chat Completions
                            let call_id = if id.contains('|') {
                                id.split('|').next().unwrap_or(id).to_string()
                            } else {
                                id.clone()
                            };
                            tool_calls.push(json!({
                                "id": call_id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": serde_json::to_string(arguments).unwrap_or_default(),
                                }
                            }));
                        }
                        ContentBlock::Thinking { .. } | ContentBlock::Image { .. } => {
                            // Skip thinking and image blocks
                        }
                    }
                }

                let content_text = if text_parts.is_empty() {
                    Value::Null
                } else {
                    json!(text_parts.join(""))
                };

                let mut msg = json!({ "role": "assistant", "content": content_text });
                if !tool_calls.is_empty() {
                    msg["tool_calls"] = json!(tool_calls);
                }
                messages.push(msg);
            }

            Message::ToolResult(tr) => {
                let text_result: String = tr
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        UserBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                // Extract call_id (strip pipe-separated item_id if present)
                let call_id = tr
                    .tool_call_id
                    .split('|')
                    .next()
                    .unwrap_or(&tr.tool_call_id);

                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": text_result,
                }));
            }
        }
    }

    messages
}

// =============================================================================
// Tool conversion
// =============================================================================

/// Convert tau `Tool` definitions to the Chat Completions function tool format.
pub fn convert_chat_tools(tools: &[Tool]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })
        })
        .collect()
}

// =============================================================================
// Stop reason mapping
// =============================================================================

pub fn map_finish_reason(reason: Option<&str>) -> StopReason {
    match reason {
        Some("stop") => StopReason::Stop,
        Some("length") => StopReason::Length,
        Some("tool_calls") | Some("function_call") => StopReason::ToolUse,
        Some("content_filter") => StopReason::Stop,
        None => StopReason::Stop,
        _ => StopReason::Stop,
    }
}

// =============================================================================
// Request body builder
// =============================================================================

/// Build the request body JSON for the OpenAI Chat Completions API.
pub fn build_chat_request_body(
    model: &Model,
    context: &Context,
    opts: &ChatRequestOptions,
) -> Value {
    let messages = convert_chat_messages(model, context);

    let mut body = json!({
        "model": model.id,
        "messages": messages,
        "stream": true,
        "stream_options": { "include_usage": true },
    });

    if let Some(max) = opts.max_tokens {
        body["max_tokens"] = json!(max);
    }
    if let Some(temp) = opts.temperature {
        body["temperature"] = json!(temp);
    }

    // Tools
    if let Some(tools) = &context.tools {
        if !tools.is_empty() {
            body["tools"] = json!(convert_chat_tools(tools));
        }
    }

    // Reasoning
    if model.reasoning {
        if let Some(effort) = &opts.reasoning_effort {
            let clamped = clamp_reasoning_effort(effort, model);
            body["reasoning_effort"] = json!(clamped);
        }
    }

    body
}

// =============================================================================
// SSE event processing state machine
// =============================================================================

/// State for tracking an in-progress tool call across SSE chunks.
struct ToolCallState {
    id: String,
    name: String,
    arguments_json: String,
    content_index: usize,
}

/// Process a list of parsed Chat Completions SSE event objects.
///
/// Chat Completions SSE is simpler than the Responses API — all data comes
/// through `choices[0].delta` with optional `usage` in the final chunk.
pub async fn process_chat_sse_events(
    events: Vec<Value>,
    output: &mut AssistantMessage,
    tx: &mut AssistantMessageEventSender,
    model: &Model,
) -> anyhow::Result<()> {
    let mut thinking_started = false;
    let mut text_started = false;
    let mut thinking_content_index: Option<usize> = None;
    let mut text_content_index: Option<usize> = None;

    // Track tool calls by their index in delta.tool_calls
    let mut tool_calls: HashMap<usize, ToolCallState> = HashMap::new();

    for event in &events {
        // Check for error object
        if let Some(err) = event.get("error") {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error");
            return Err(anyhow::anyhow!("API error: {}", msg));
        }

        let choices = match event.get("choices").and_then(|c| c.as_array()) {
            Some(c) if !c.is_empty() => c,
            _ => {
                // Final chunk may have only usage, no choices
                extract_usage(event, output, model);
                continue;
            }
        };

        let choice = &choices[0];
        let delta = match choice.get("delta") {
            Some(d) => d,
            None => continue,
        };
        let finish_reason = choice.get("finish_reason").and_then(|r| r.as_str());

        // 1. Reasoning / thinking content
        let reasoning_text = delta
            .get("reasoning_content")
            .or_else(|| delta.get("reasoning"))
            .or_else(|| delta.get("reasoning_text"))
            .and_then(|v| v.as_str());

        if let Some(text) = reasoning_text {
            if !thinking_started {
                let block = ContentBlock::Thinking {
                    thinking: String::new(),
                    thinking_signature: None,
                    redacted: None,
                };
                output.content.push(block);
                let idx = output.content.len() - 1;
                thinking_content_index = Some(idx);
                thinking_started = true;
                tx.push(AssistantMessageEvent::ThinkingStart {
                    content_index: idx,
                    partial: output.clone(),
                });
            }
            if let Some(idx) = thinking_content_index {
                if let Some(ContentBlock::Thinking { thinking, .. }) = output.content.get_mut(idx) {
                    thinking.push_str(text);
                }
                tx.push(AssistantMessageEvent::ThinkingDelta {
                    content_index: idx,
                    delta: text.to_string(),
                    partial: output.clone(),
                });
            }
        }

        // 2. Text content
        if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
            // End thinking if transitioning to text
            if thinking_started && !text_started {
                if let Some(idx) = thinking_content_index {
                    let content = if let Some(ContentBlock::Thinking { thinking, .. }) =
                        output.content.get(idx)
                    {
                        thinking.clone()
                    } else {
                        String::new()
                    };
                    tx.push(AssistantMessageEvent::ThinkingEnd {
                        content_index: idx,
                        content,
                        partial: output.clone(),
                    });
                }
            }

            if !text_started {
                let block = ContentBlock::Text {
                    text: String::new(),
                    text_signature: None,
                };
                output.content.push(block);
                let idx = output.content.len() - 1;
                text_content_index = Some(idx);
                text_started = true;
                tx.push(AssistantMessageEvent::TextStart {
                    content_index: idx,
                    partial: output.clone(),
                });
            }
            if let Some(idx) = text_content_index {
                if let Some(ContentBlock::Text {
                    text: ref mut t, ..
                }) = output.content.get_mut(idx)
                {
                    t.push_str(text);
                }
                tx.push(AssistantMessageEvent::TextDelta {
                    content_index: idx,
                    delta: text.to_string(),
                    partial: output.clone(),
                });
            }
        }

        // 3. Tool calls
        if let Some(tc_array) = delta.get("tool_calls").and_then(|t| t.as_array()) {
            // End text block if transitioning to tool calls
            if text_started {
                if let Some(idx) = text_content_index {
                    let content =
                        if let Some(ContentBlock::Text { text, .. }) = output.content.get(idx) {
                            text.clone()
                        } else {
                            String::new()
                        };
                    tx.push(AssistantMessageEvent::TextEnd {
                        content_index: idx,
                        content,
                        partial: output.clone(),
                    });
                    text_started = false;
                }
            }

            for tc in tc_array {
                let tc_index = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;

                // New tool call (has id + function.name)
                if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                    let name = tc
                        .pointer("/function/name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string();

                    let block = ContentBlock::ToolCall {
                        id: id.to_string(),
                        name: name.clone(),
                        arguments: HashMap::new(),
                        thought_signature: None,
                    };
                    output.content.push(block);
                    let content_idx = output.content.len() - 1;

                    tool_calls.insert(
                        tc_index,
                        ToolCallState {
                            id: id.to_string(),
                            name,
                            arguments_json: String::new(),
                            content_index: content_idx,
                        },
                    );

                    tx.push(AssistantMessageEvent::ToolCallStart {
                        content_index: content_idx,
                        partial: output.clone(),
                    });
                }

                // Argument delta
                if let Some(args_delta) = tc.pointer("/function/arguments").and_then(|a| a.as_str())
                {
                    if let Some(state) = tool_calls.get_mut(&tc_index) {
                        state.arguments_json.push_str(args_delta);
                        tx.push(AssistantMessageEvent::ToolCallDelta {
                            content_index: state.content_index,
                            delta: args_delta.to_string(),
                            partial: output.clone(),
                        });
                    }
                }
            }
        }

        // 4. Finish reason — finalize everything
        if let Some(reason) = finish_reason {
            // End thinking if still open
            if thinking_started {
                if let Some(idx) = thinking_content_index {
                    let content = if let Some(ContentBlock::Thinking { thinking, .. }) =
                        output.content.get(idx)
                    {
                        thinking.clone()
                    } else {
                        String::new()
                    };
                    tx.push(AssistantMessageEvent::ThinkingEnd {
                        content_index: idx,
                        content,
                        partial: output.clone(),
                    });
                    thinking_started = false;
                }
            }

            // End text if still open
            if text_started {
                if let Some(idx) = text_content_index {
                    let content =
                        if let Some(ContentBlock::Text { text, .. }) = output.content.get(idx) {
                            text.clone()
                        } else {
                            String::new()
                        };
                    tx.push(AssistantMessageEvent::TextEnd {
                        content_index: idx,
                        content,
                        partial: output.clone(),
                    });
                }
            }

            // Finalize all tool calls
            for state in tool_calls.values() {
                let arguments: HashMap<String, Value> =
                    serde_json::from_str(&state.arguments_json).unwrap_or_default();

                let tool_call = ContentBlock::ToolCall {
                    id: state.id.clone(),
                    name: state.name.clone(),
                    arguments,
                    thought_signature: None,
                };

                if let Some(block) = output.content.get_mut(state.content_index) {
                    *block = tool_call.clone();
                }

                tx.push(AssistantMessageEvent::ToolCallEnd {
                    content_index: state.content_index,
                    tool_call,
                    partial: output.clone(),
                });
            }

            // Extract usage from this chunk
            extract_usage(event, output, model);

            // Map stop reason
            output.stop_reason = map_finish_reason(Some(reason));
        }
    }

    Ok(())
}

/// Extract usage information from a Chat Completions SSE chunk.
fn extract_usage(event: &Value, output: &mut AssistantMessage, model: &Model) {
    if let Some(usage_obj) = event.get("usage") {
        let prompt_tokens = usage_obj
            .get("prompt_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let completion_tokens = usage_obj
            .get("completion_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let total_tokens = usage_obj
            .get("total_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cached_tokens = usage_obj
            .pointer("/prompt_tokens_details/cached_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        output.usage = crate::types::Usage {
            input: prompt_tokens.saturating_sub(cached_tokens),
            output: completion_tokens,
            cache_read: cached_tokens,
            cache_write: 0,
            total_tokens,
            cost: crate::types::Cost::default(),
        };

        calculate_cost(model, &mut output.usage);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AssistantMessage, Model, ModelCost, StopReason, Usage};

    fn test_model() -> Model {
        Model {
            id: "test-model".into(),
            name: "Test Model".into(),
            api: "openai-chat".into(),
            provider: "openrouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            reasoning: false,
            input: vec!["text".into()],
            cost: ModelCost {
                input: 0.0,
                output: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
            },
            context_window: 128000,
            max_tokens: 4096,
            headers: None,
            compat: None,
        }
    }

    #[test]
    fn test_convert_system_prompt() {
        let model = test_model();
        let context = Context {
            system_prompt: Some("You are helpful.".into()),
            messages: vec![],
            tools: None,
        };
        let msgs = convert_chat_messages(&model, &context);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "You are helpful.");
    }

    #[test]
    fn test_convert_user_text() {
        let model = test_model();
        let context = Context {
            system_prompt: None,
            messages: vec![Message::User(crate::types::UserMessage::new("hello"))],
            tools: None,
        };
        let msgs = convert_chat_messages(&model, &context);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"], "hello");
    }

    #[test]
    fn test_convert_tool_calls_strips_pipe_id() {
        let model = test_model();
        let assistant = AssistantMessage {
            role: "assistant".into(),
            content: vec![ContentBlock::ToolCall {
                id: "call_abc|fc_xyz".into(),
                name: "bash".into(),
                arguments: {
                    let mut m = HashMap::new();
                    m.insert("cmd".into(), json!("ls"));
                    m
                },
                thought_signature: None,
            }],
            api: "openai-chat".into(),
            provider: "openrouter".into(),
            model: "test".into(),
            usage: Usage::default(),
            stop_reason: StopReason::ToolUse,
            error_message: None,
            timestamp: 0,
        };
        let context = Context {
            system_prompt: None,
            messages: vec![Message::Assistant(assistant)],
            tools: None,
        };
        let msgs = convert_chat_messages(&model, &context);
        assert_eq!(msgs[0]["tool_calls"][0]["id"], "call_abc");
    }

    #[test]
    fn test_convert_tool_result() {
        let model = test_model();
        let context = Context {
            system_prompt: None,
            messages: vec![Message::ToolResult(crate::types::ToolResultMessage {
                role: "toolResult".into(),
                tool_call_id: "call_abc|fc_xyz".into(),
                tool_name: "bash".into(),
                content: vec![UserBlock::Text {
                    text: "output".into(),
                }],
                details: None,
                is_error: false,
                timestamp: 0,
            })],
            tools: None,
        };
        let msgs = convert_chat_messages(&model, &context);
        assert_eq!(msgs[0]["role"], "tool");
        assert_eq!(msgs[0]["tool_call_id"], "call_abc");
        assert_eq!(msgs[0]["content"], "output");
    }

    #[test]
    fn test_convert_chat_tools_format() {
        let tools = vec![Tool {
            name: "bash".into(),
            description: "Run a command".into(),
            parameters: json!({"type": "object", "properties": {"cmd": {"type": "string"}}}),
        }];
        let result = convert_chat_tools(&tools);
        assert_eq!(result[0]["type"], "function");
        assert_eq!(result[0]["function"]["name"], "bash");
        assert_eq!(result[0]["function"]["description"], "Run a command");
    }

    #[test]
    fn test_map_finish_reasons() {
        assert_eq!(map_finish_reason(Some("stop")), StopReason::Stop);
        assert_eq!(map_finish_reason(Some("length")), StopReason::Length);
        assert_eq!(map_finish_reason(Some("tool_calls")), StopReason::ToolUse);
        assert_eq!(
            map_finish_reason(Some("function_call")),
            StopReason::ToolUse
        );
        assert_eq!(map_finish_reason(None), StopReason::Stop);
    }

    #[test]
    fn test_build_request_body_includes_stream_options() {
        let model = test_model();
        let context = Context {
            system_prompt: Some("sys".into()),
            messages: vec![],
            tools: None,
        };
        let opts = ChatRequestOptions::default();
        let body = build_chat_request_body(&model, &context, &opts);
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
        assert_eq!(body["model"], "test-model");
    }
}
