//! Shared helpers for the OpenAI Responses provider:
//! message conversion, tool call ID normalization, SSE event processing, cost helpers.
//!
//! Mirrors: packages/ai/src/providers/openai-responses-shared.ts

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::models::{calculate_cost, supports_xhigh};
use crate::stream::AssistantMessageEventSender;
use crate::types::{
    AssistantMessage, AssistantMessageEvent, ContentBlock, Context, Message, Model, StopReason,
    Tool, Usage, UserBlock, UserContent,
};

/// Providers whose tool call IDs must be normalized to OpenAI pipe-separated format.
pub const OPENAI_TOOL_CALL_PROVIDERS: &[&str] = &["openai", "openai-codex", "opencode"];

// =============================================================================
// Tool call ID normalization
// =============================================================================

/// Normalize a pipe-separated tool call ID for the OpenAI Responses API.
///
/// OpenAI encodes tool call IDs as `{call_id}|{item_id}`.
/// Rules:
/// - Only normalize for known OpenAI providers
/// - If no pipe, pass through unchanged (cross-provider IDs)
/// - Replace invalid chars with `_`
/// - `item_id` must start with "fc"
/// - Both parts truncated to 64 chars, trailing `_` stripped
pub fn normalize_tool_call_id(id: &str, provider: &str) -> String {
    if !OPENAI_TOOL_CALL_PROVIDERS.contains(&provider) {
        return id.to_string();
    }
    if !id.contains('|') {
        return id.to_string();
    }

    let mut iter = id.splitn(2, '|');
    let call_id = iter.next().unwrap_or("");
    let item_id = iter.next().unwrap_or("");

    let sanitize = |s: &str| -> String {
        s.chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
            .collect()
    };

    let sanitized_call_id = sanitize(call_id);
    let sanitized_item_id = sanitize(item_id);

    // item_id must start with "fc"
    let sanitized_item_id = if !sanitized_item_id.starts_with("fc") {
        format!("fc_{}", sanitized_item_id)
    } else {
        sanitized_item_id
    };

    // Truncate to 64 chars and strip trailing underscores
    let normalized_call_id: String = sanitized_call_id.chars().take(64).collect();
    let normalized_call_id = normalized_call_id.trim_end_matches('_').to_string();

    let normalized_item_id: String = sanitized_item_id.chars().take(64).collect();
    let normalized_item_id = normalized_item_id.trim_end_matches('_').to_string();

    format!("{}|{}", normalized_call_id, normalized_item_id)
}

// =============================================================================
// Utilities
// =============================================================================

/// Fast hash to shorten long strings (mirrors the TS shortHash function).
fn short_hash(s: &str) -> String {
    let mut h1: u32 = 0xdeadbeef;
    let mut h2: u32 = 0x41c6ce57;
    for ch in s.chars() {
        let c = ch as u32;
        h1 = h1.wrapping_mul(2654435761).wrapping_add(h1 ^ c);
        h2 = h2.wrapping_mul(1597334677).wrapping_add(h2 ^ c);
    }
    h1 = (h1 ^ (h1 >> 16))
        .wrapping_mul(2246822507)
        ^ (h2 ^ (h2 >> 13)).wrapping_mul(3266489909);
    h2 = (h2 ^ (h2 >> 16))
        .wrapping_mul(2246822507)
        ^ (h1 ^ (h1 >> 13)).wrapping_mul(3266489909);
    format!("{}{}", radix36(h2), radix36(h1))
}

fn radix36(n: u32) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let mut result = String::new();
    let mut n = n;
    while n > 0 {
        let digit = (n % 36) as u8;
        result.push(if digit < 10 {
            (b'0' + digit) as char
        } else {
            (b'a' + digit - 10) as char
        });
        n /= 36;
    }
    result.chars().rev().collect()
}

// =============================================================================
// Message conversion
// =============================================================================

/// Convert tau `Context` to an OpenAI Responses API input array.
///
/// Returns a `Vec<Value>` of input items ready to serialize.
pub fn convert_responses_messages(model: &Model, context: &Context) -> Vec<Value> {
    let mut messages: Vec<Value> = Vec::new();

    // System / developer prompt
    if let Some(ref sys) = context.system_prompt {
        let role = if model.reasoning { "developer" } else { "system" };
        messages.push(json!({ "role": role, "content": sys }));
    }

    let mut msg_index: usize = 0;

    for msg in &context.messages {
        match msg {
            Message::User(u) => {
                let content = match &u.content {
                    UserContent::Text(t) => {
                        vec![json!({ "type": "input_text", "text": t })]
                    }
                    UserContent::Blocks(blocks) => {
                        let mut parts = Vec::new();
                        for block in blocks {
                            match block {
                                UserBlock::Text { text } => {
                                    parts.push(json!({ "type": "input_text", "text": text }));
                                }
                                UserBlock::Image { data, mime_type } => {
                                    // Only include images if model supports them
                                    if model.input.iter().any(|i| i == "image") {
                                        parts.push(json!({
                                            "type": "input_image",
                                            "detail": "auto",
                                            "image_url": format!("data:{};base64,{}", mime_type, data),
                                        }));
                                    }
                                }
                            }
                        }
                        if parts.is_empty() {
                            msg_index += 1;
                            continue;
                        }
                        parts
                    }
                };
                messages.push(json!({ "role": "user", "content": content }));
            }

            Message::Assistant(a) => {
                let is_different_model = a.model != model.id
                    && a.provider == model.provider
                    && a.api == model.api;

                let mut output: Vec<Value> = Vec::new();

                for block in &a.content {
                    match block {
                        ContentBlock::Thinking { thinking_signature: Some(sig), .. } => {
                            // Restore as a reasoning item using the stored signature JSON
                            if let Ok(reasoning_item) = serde_json::from_str::<Value>(sig) {
                                output.push(reasoning_item);
                            }
                        }
                        ContentBlock::Thinking { .. } => {
                            // No signature — skip (e.g. cross-provider thinking)
                        }
                        ContentBlock::Text { text, text_signature } => {
                            let id = match text_signature {
                                Some(sig) if sig.len() <= 64 => sig.clone(),
                                Some(sig) => format!("msg_{}", short_hash(sig)),
                                None => format!("msg_{}", msg_index),
                            };
                            output.push(json!({
                                "type": "message",
                                "role": "assistant",
                                "content": [{ "type": "output_text", "text": text, "annotations": [] }],
                                "status": "completed",
                                "id": id,
                            }));
                        }
                        ContentBlock::ToolCall { id, name, arguments, .. } => {
                            let normalized = normalize_tool_call_id(id, &a.provider);
                            let mut call_id_str = normalized.clone();
                            let mut item_id_opt: Option<String> = None;

                            if normalized.contains('|') {
                                let mut p = normalized.splitn(2, '|');
                                call_id_str = p.next().unwrap_or("").to_string();
                                let raw_item_id = p.next().unwrap_or("").to_string();

                                // For different-model messages, omit item_id to skip pairing validation
                                if is_different_model && raw_item_id.starts_with("fc_") {
                                    item_id_opt = None;
                                } else {
                                    item_id_opt = Some(raw_item_id);
                                }
                            }

                            let mut item = json!({
                                "type": "function_call",
                                "call_id": call_id_str,
                                "name": name,
                                "arguments": serde_json::to_string(arguments).unwrap_or_default(),
                            });
                            if let Some(item_id) = item_id_opt {
                                item["id"] = json!(item_id);
                            }
                            output.push(item);
                        }
                        ContentBlock::Image { .. } => {
                            // Images in assistant content not sent back
                        }
                    }
                }

                if !output.is_empty() {
                    messages.extend(output);
                }
            }

            Message::ToolResult(tr) => {
                // Extract text content
                let text_result: String = tr
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        UserBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                let has_images = tr.content.iter().any(|c| matches!(c, UserBlock::Image { .. }));
                let has_text = !text_result.is_empty();

                // call_id is the part before `|`
                let call_id = tr.tool_call_id.split('|').next().unwrap_or(&tr.tool_call_id);

                messages.push(json!({
                    "type": "function_call_output",
                    "call_id": call_id,
                    "output": if has_text { text_result.clone() } else { "(see attached image)".to_string() },
                }));

                // If images and model supports them, add a follow-up user message
                if has_images && model.input.iter().any(|i| i == "image") {
                    let mut content_parts = vec![json!({
                        "type": "input_text",
                        "text": "Attached image(s) from tool result:",
                    })];
                    for block in &tr.content {
                        if let UserBlock::Image { data, mime_type } = block {
                            content_parts.push(json!({
                                "type": "input_image",
                                "detail": "auto",
                                "image_url": format!("data:{};base64,{}", mime_type, data),
                            }));
                        }
                    }
                    messages.push(json!({ "role": "user", "content": content_parts }));
                }
            }
        }
        msg_index += 1;
    }

    messages
}

/// Convert tau `Tool` definitions to the OpenAI function tool format.
pub fn convert_responses_tools(tools: &[Tool]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
                "strict": false,
            })
        })
        .collect()
}

// =============================================================================
// Stop reason mapping
// =============================================================================

pub fn map_stop_reason(status: Option<&str>) -> StopReason {
    match status {
        None | Some("completed") | Some("in_progress") | Some("queued") => StopReason::Stop,
        Some("incomplete") => StopReason::Length,
        Some("failed") | Some("cancelled") => StopReason::Error,
        _ => StopReason::Stop,
    }
}

// =============================================================================
// Service tier pricing
// =============================================================================

pub fn service_tier_multiplier(service_tier: Option<&str>) -> f64 {
    match service_tier {
        Some("flex") => 0.5,
        Some("priority") => 2.0,
        _ => 1.0,
    }
}

pub fn apply_service_tier_pricing(usage: &mut Usage, service_tier: Option<&str>) {
    let multiplier = service_tier_multiplier(service_tier);
    if (multiplier - 1.0).abs() < f64::EPSILON {
        return;
    }
    usage.cost.input *= multiplier;
    usage.cost.output *= multiplier;
    usage.cost.cache_read *= multiplier;
    usage.cost.cache_write *= multiplier;
    usage.cost.total =
        usage.cost.input + usage.cost.output + usage.cost.cache_read + usage.cost.cache_write;
}

// =============================================================================
// Reasoning effort helpers
// =============================================================================

/// Clamp reasoning effort: "xhigh" → "high" for models that don't support xhigh.
pub fn clamp_reasoning_effort(effort: &str, model: &Model) -> &'static str {
    if effort == "xhigh" && !supports_xhigh(model) {
        "high"
    } else {
        match effort {
            "minimal" => "minimal",
            "low" => "low",
            "medium" => "medium",
            "high" => "high",
            "xhigh" => "xhigh",
            _ => "medium",
        }
    }
}

// =============================================================================
// SSE event processing state machine
// =============================================================================

/// Internal state for the SSE processing loop.
#[derive(Debug)]
enum CurrentItemType {
    Reasoning,
    Message,
    FunctionCall,
}

/// Process a list of parsed OpenAI SSE event objects.
///
/// Mutates `output` and pushes `AssistantMessageEvent`s via `tx`.
/// Public so integration tests can drive it directly.
pub async fn process_sse_events(
    events: Vec<Value>,
    output: &mut AssistantMessage,
    tx: &mut AssistantMessageEventSender,
    model: &Model,
    service_tier: Option<&str>,
) -> anyhow::Result<()> {
    let mut current_item_type: Option<CurrentItemType> = None;
    // For reasoning items: accumulate summary text and the raw item JSON
    let mut reasoning_summary: String = String::new();
    let mut reasoning_item_raw: Option<Value> = None;
    // For tool calls: accumulate partial JSON
    let mut tool_partial_json: String = String::new();

    for event in events {
        let event_type = match event.get("type").and_then(|t| t.as_str()) {
            Some(t) => t.to_string(),
            None => continue,
        };

        match event_type.as_str() {
            "response.output_item.added" => {
                let item = &event["item"];
                match item.get("type").and_then(|t| t.as_str()) {
                    Some("reasoning") => {
                        current_item_type = Some(CurrentItemType::Reasoning);
                        reasoning_summary = String::new();
                        reasoning_item_raw = Some(item.clone());
                        let block = ContentBlock::Thinking {
                            thinking: String::new(),
                            thinking_signature: None,
                            redacted: None,
                        };
                        output.content.push(block);
                        let idx = output.content.len() - 1;
                        tx.push(AssistantMessageEvent::ThinkingStart {
                            content_index: idx,
                            partial: output.clone(),
                        });
                    }
                    Some("message") => {
                        current_item_type = Some(CurrentItemType::Message);
                        let block = ContentBlock::Text {
                            text: String::new(),
                            text_signature: None,
                        };
                        output.content.push(block);
                        let idx = output.content.len() - 1;
                        tx.push(AssistantMessageEvent::TextStart {
                            content_index: idx,
                            partial: output.clone(),
                        });
                    }
                    Some("function_call") => {
                        current_item_type = Some(CurrentItemType::FunctionCall);
                        tool_partial_json = item
                            .get("arguments")
                            .and_then(|a| a.as_str())
                            .unwrap_or("")
                            .to_string();
                        let call_id = item
                            .get("call_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let item_id = item
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let tool_id = format!("{}|{}", call_id, item_id);
                        let name = item
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let block = ContentBlock::ToolCall {
                            id: tool_id,
                            name,
                            arguments: HashMap::new(),
                            thought_signature: None,
                        };
                        output.content.push(block);
                        let idx = output.content.len() - 1;
                        tx.push(AssistantMessageEvent::ToolCallStart {
                            content_index: idx,
                            partial: output.clone(),
                        });
                    }
                    _ => {}
                }
            }

            "response.reasoning_summary_part.added" => {
                // Track that a new summary part is starting
            }

            "response.reasoning_summary_text.delta" => {
                if matches!(current_item_type, Some(CurrentItemType::Reasoning)) {
                    let delta = event
                        .get("delta")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string();
                    reasoning_summary.push_str(&delta);

                    // Update the last thinking block
                    if let Some(ContentBlock::Thinking { thinking, .. }) =
                        output.content.last_mut()
                    {
                        *thinking = reasoning_summary.clone();
                    }

                    let idx = output.content.len().saturating_sub(1);
                    tx.push(AssistantMessageEvent::ThinkingDelta {
                        content_index: idx,
                        delta,
                        partial: output.clone(),
                    });
                }
            }

            "response.reasoning_summary_part.done" => {
                if matches!(current_item_type, Some(CurrentItemType::Reasoning)) {
                    // Add paragraph separator between summary parts
                    let delta = "\n\n".to_string();
                    reasoning_summary.push_str(&delta);
                    if let Some(ContentBlock::Thinking { thinking, .. }) =
                        output.content.last_mut()
                    {
                        *thinking = reasoning_summary.clone();
                    }
                    let idx = output.content.len().saturating_sub(1);
                    tx.push(AssistantMessageEvent::ThinkingDelta {
                        content_index: idx,
                        delta,
                        partial: output.clone(),
                    });
                }
            }

            "response.content_part.added" => {
                // Content part tracking is handled via output_text.delta
            }

            "response.output_text.delta" | "response.refusal.delta" => {
                if matches!(current_item_type, Some(CurrentItemType::Message)) {
                    let delta = event
                        .get("delta")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string();

                    if let Some(ContentBlock::Text { text, .. }) = output.content.last_mut() {
                        text.push_str(&delta);
                    }

                    let idx = output.content.len().saturating_sub(1);
                    tx.push(AssistantMessageEvent::TextDelta {
                        content_index: idx,
                        delta,
                        partial: output.clone(),
                    });
                }
            }

            "response.function_call_arguments.delta" => {
                if matches!(current_item_type, Some(CurrentItemType::FunctionCall)) {
                    let delta = event
                        .get("delta")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string();
                    tool_partial_json.push_str(&delta);

                    let idx = output.content.len().saturating_sub(1);
                    tx.push(AssistantMessageEvent::ToolCallDelta {
                        content_index: idx,
                        delta,
                        partial: output.clone(),
                    });
                }
            }

            "response.function_call_arguments.done" => {
                if matches!(current_item_type, Some(CurrentItemType::FunctionCall)) {
                    let arguments_str = event
                        .get("arguments")
                        .and_then(|a| a.as_str())
                        .unwrap_or(&tool_partial_json);
                    tool_partial_json = arguments_str.to_string();
                }
            }

            "response.output_item.done" => {
                let item = &event["item"];
                match item.get("type").and_then(|t| t.as_str()) {
                    Some("reasoning") => {
                        if matches!(current_item_type, Some(CurrentItemType::Reasoning)) {
                            // Build final thinking text from summary array
                            let final_thinking: String = item
                                .get("summary")
                                .and_then(|s| s.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|part| {
                                            part.get("text").and_then(|t| t.as_str())
                                        })
                                        .collect::<Vec<_>>()
                                        .join("\n\n")
                                })
                                .unwrap_or_else(|| reasoning_summary.clone());

                            let signature = serde_json::to_string(item).ok();

                            if let Some(ContentBlock::Thinking {
                                thinking,
                                thinking_signature,
                                ..
                            }) = output.content.last_mut()
                            {
                                *thinking = final_thinking.clone();
                                *thinking_signature = signature;
                            }

                            let idx = output.content.len().saturating_sub(1);
                            tx.push(AssistantMessageEvent::ThinkingEnd {
                                content_index: idx,
                                content: final_thinking,
                                partial: output.clone(),
                            });
                            current_item_type = None;
                        }
                    }
                    Some("message") => {
                        if matches!(current_item_type, Some(CurrentItemType::Message)) {
                            let final_text: String = item
                                .get("content")
                                .and_then(|c| c.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|part| {
                                            match part.get("type").and_then(|t| t.as_str()) {
                                                Some("output_text") => {
                                                    part.get("text").and_then(|t| t.as_str())
                                                }
                                                Some("refusal") => {
                                                    part.get("refusal").and_then(|t| t.as_str())
                                                }
                                                _ => None,
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                        .join("")
                                })
                                .unwrap_or_default();

                            let msg_id = item
                                .get("id")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());

                            if let Some(ContentBlock::Text { text, text_signature }) =
                                output.content.last_mut()
                            {
                                *text = final_text.clone();
                                *text_signature = msg_id;
                            }

                            let idx = output.content.len().saturating_sub(1);
                            tx.push(AssistantMessageEvent::TextEnd {
                                content_index: idx,
                                content: final_text,
                                partial: output.clone(),
                            });
                            current_item_type = None;
                        }
                    }
                    Some("function_call") => {
                        let args_str = item
                            .get("arguments")
                            .and_then(|a| a.as_str())
                            .unwrap_or(&tool_partial_json);
                        let arguments: HashMap<String, Value> =
                            serde_json::from_str(args_str).unwrap_or_default();

                        let call_id = item
                            .get("call_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let item_id = item
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let tool_id = format!("{}|{}", call_id, item_id);
                        let name = item
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();

                        let tool_call = ContentBlock::ToolCall {
                            id: tool_id.clone(),
                            name: name.clone(),
                            arguments: arguments.clone(),
                            thought_signature: None,
                        };

                        // Update the placeholder block that was added on output_item.added
                        if let Some(last) = output.content.last_mut() {
                            *last = tool_call.clone();
                        }

                        let idx = output.content.len().saturating_sub(1);
                        tx.push(AssistantMessageEvent::ToolCallEnd {
                            content_index: idx,
                            tool_call,
                            partial: output.clone(),
                        });
                        current_item_type = None;
                        tool_partial_json = String::new();
                    }
                    _ => {}
                }
            }

            "response.completed" => {
                let response = &event["response"];
                if let Some(usage_obj) = response.get("usage") {
                    let input_tokens = usage_obj
                        .get("input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let output_tokens = usage_obj
                        .get("output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let total_tokens = usage_obj
                        .get("total_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let cached_tokens = usage_obj
                        .pointer("/input_tokens_details/cached_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);

                    output.usage = Usage {
                        // OpenAI includes cached tokens in input_tokens, subtract to get non-cached
                        input: input_tokens.saturating_sub(cached_tokens),
                        output: output_tokens,
                        cache_read: cached_tokens,
                        cache_write: 0,
                        total_tokens,
                        cost: crate::types::Cost::default(),
                    };
                }

                calculate_cost(model, &mut output.usage);

                // Apply service tier: prefer the tier from the response, fall back to param
                let response_tier = response
                    .get("service_tier")
                    .and_then(|v| v.as_str());
                let effective_tier = response_tier.or(service_tier);
                apply_service_tier_pricing(&mut output.usage, effective_tier);

                output.stop_reason = map_stop_reason(
                    response.get("status").and_then(|v| v.as_str()),
                );

                // If any tool call is present and stop reason is stop → toolUse
                if output.content.iter().any(|b| matches!(b, ContentBlock::ToolCall { .. }))
                    && output.stop_reason == StopReason::Stop
                {
                    output.stop_reason = StopReason::ToolUse;
                }
            }

            "error" => {
                let code = event
                    .get("code")
                    .and_then(|c| c.as_i64())
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                let msg = event
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error");
                return Err(anyhow::anyhow!("Error Code {}: {}", code, msg));
            }

            "response.failed" => {
                return Err(anyhow::anyhow!("Response failed"));
            }

            // All other event types are silently skipped
            _ => {}
        }

        let _ = reasoning_item_raw; // suppress unused warning
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_no_pipe_passthrough() {
        assert_eq!(normalize_tool_call_id("call_123", "openai"), "call_123");
    }

    #[test]
    fn normalize_cross_provider_passthrough() {
        assert_eq!(normalize_tool_call_id("toolu_abc|extra", "anthropic"), "toolu_abc|extra");
    }

    #[test]
    fn normalize_adds_fc_prefix_to_item_id() {
        let result = normalize_tool_call_id("call_abc|item_xyz", "openai");
        let parts: Vec<&str> = result.split('|').collect();
        assert_eq!(parts[0], "call_abc");
        assert!(parts[1].starts_with("fc"), "item_id must start with fc, got: {}", parts[1]);
    }

    #[test]
    fn normalize_keeps_existing_fc_prefix() {
        let result = normalize_tool_call_id("call_abc|fc_xyz", "openai");
        let parts: Vec<&str> = result.split('|').collect();
        assert_eq!(parts[1], "fc_xyz");
    }

    #[test]
    fn normalize_replaces_invalid_chars() {
        let result = normalize_tool_call_id("call+abc|item/xyz", "openai");
        assert!(!result.contains('+'));
        assert!(!result.contains('/'));
    }

    #[test]
    fn normalize_truncates_to_64_chars() {
        let long_call = "a".repeat(100);
        let long_item = "fc_".to_string() + &"b".repeat(100);
        let id = format!("{}|{}", long_call, long_item);
        let result = normalize_tool_call_id(&id, "openai");
        let parts: Vec<&str> = result.split('|').collect();
        assert!(parts[0].len() <= 64, "call_id too long: {}", parts[0].len());
        assert!(parts[1].len() <= 64, "item_id too long: {}", parts[1].len());
    }

    #[test]
    fn normalize_strips_trailing_underscores() {
        // After truncation, trailing underscores should be stripped
        // Create a string that will have trailing underscores after truncation
        let call_id = "a".repeat(63) + "_";
        let item_id = format!("fc_{}{}", "b".repeat(61), "__");
        let id = format!("{}|{}", call_id, item_id);
        let result = normalize_tool_call_id(&id, "openai");
        let parts: Vec<&str> = result.split('|').collect();
        assert!(!parts[0].ends_with('_'), "call_id should not end with _");
        assert!(!parts[1].ends_with('_'), "item_id should not end with _");
    }
}
