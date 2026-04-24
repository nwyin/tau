//! OpenAI Responses API provider implementation.
//!
//! Mirrors: packages/ai/src/providers/openai-responses.ts

use std::collections::HashMap;

use anyhow::Result;
use serde_json::{json, Value};

use crate::providers::openai_responses_shared::{
    clamp_reasoning_effort, convert_responses_messages, convert_responses_tools, process_sse_events,
};
use crate::providers::sse::{self, SseStop};
use crate::providers::ApiProvider;
use crate::stream::{assistant_message_event_stream, error_stream, AssistantMessageEventStream};
use crate::types::{
    AssistantMessage, AssistantMessageEvent, CacheRetention, Context, Cost, Model,
    SimpleStreamOptions, StopReason, StreamOptions, ThinkingLevel, Usage,
};

// =============================================================================
// Provider struct
// =============================================================================

pub struct OpenAIResponsesProvider {
    client: reqwest::Client,
}

impl OpenAIResponsesProvider {
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
        let env_var = "OPENAI_API_KEY";
        std::env::var(env_var).map_err(|_| {
            anyhow::anyhow!(
                "No API key for provider '{}'. Set {} or pass api_key.",
                model.provider,
                env_var
            )
        })
    }
}

impl Default for OpenAIResponsesProvider {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Request body builder
// =============================================================================

/// Options for building the OpenAI Responses request body.
#[derive(Debug, Default, Clone)]
pub struct OpenAIRequestOptions {
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub session_id: Option<String>,
    pub cache_retention: Option<CacheRetention>,
    pub service_tier: Option<String>,
    pub reasoning_effort: Option<String>,
    pub reasoning_summary: Option<String>,
    pub extra_headers: Option<HashMap<String, String>>,
}

/// Build the request body JSON for the OpenAI Responses API.
pub fn build_request_body(model: &Model, context: &Context, opts: &OpenAIRequestOptions) -> Value {
    let messages = convert_responses_messages(model, context);

    // Resolve cache retention (default "short")
    let cache_retention = opts
        .cache_retention
        .as_ref()
        .unwrap_or(&CacheRetention::Short);

    let prompt_cache_key = if *cache_retention != CacheRetention::None {
        opts.session_id.clone()
    } else {
        None
    };

    let prompt_cache_retention: Option<&str> =
        if *cache_retention == CacheRetention::Long && model.base_url.contains("api.openai.com") {
            Some("24h")
        } else {
            None
        };

    let mut body = json!({
        "model": model.id,
        "input": messages,
        "stream": true,
        "store": false,
    });

    // ChatGPT backend requires a top-level `instructions` field for the system prompt.
    if model.base_url.contains("chatgpt.com") {
        if let Some(ref sys) = context.system_prompt {
            body["instructions"] = json!(sys);
        }
    }

    if let Some(key) = prompt_cache_key {
        body["prompt_cache_key"] = json!(key);
    }
    if let Some(retention) = prompt_cache_retention {
        body["prompt_cache_retention"] = json!(retention);
    }
    if let Some(max) = opts.max_tokens {
        body["max_output_tokens"] = json!(max);
    }
    if let Some(temp) = opts.temperature {
        body["temperature"] = json!(temp);
    }
    if let Some(tier) = &opts.service_tier {
        body["service_tier"] = json!(tier);
    }

    // Tools
    if let Some(tools) = &context.tools {
        if !tools.is_empty() {
            body["tools"] = json!(convert_responses_tools(tools));
        }
    }

    // Reasoning config
    if model.reasoning {
        if let Some(effort) = &opts.reasoning_effort {
            let clamped = clamp_reasoning_effort(effort, model);
            let summary = opts.reasoning_summary.as_deref().unwrap_or("auto");
            body["reasoning"] = json!({ "effort": clamped, "summary": summary });
            body["include"] = json!(["reasoning.encrypted_content"]);
        } else {
            // gpt-5 models need an explicit "no reasoning" hint when effort unset
            if model.name.starts_with("gpt-5") {
                let input = body["input"].as_array_mut().unwrap();
                input.push(json!({
                    "role": "developer",
                    "content": [{ "type": "input_text", "text": "# Juice: 0 !important" }],
                }));
            }
        }
    }

    body
}

// =============================================================================
// Core streaming function
// =============================================================================

/// Stream a response from the OpenAI Responses API.
///
/// Spawns a Tokio task internally; returns the stream handle immediately.
fn stream_openai_responses(
    client: reqwest::Client,
    model: Model,
    context: Context,
    api_key: String,
    opts: OpenAIRequestOptions,
) -> AssistantMessageEventStream {
    let (mut tx, stream) = assistant_message_event_stream();

    tokio::spawn(async move {
        let _permit = crate::concurrency::acquire().await;

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
        let url = format!("{}/responses", model.base_url);

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());
        headers.insert("Accept", "text/event-stream".parse().unwrap());

        if let Some(model_headers) = &model.headers {
            for (k, v) in model_headers {
                if let (Ok(name), Ok(val)) = (
                    k.parse::<reqwest::header::HeaderName>(),
                    v.parse::<reqwest::header::HeaderValue>(),
                ) {
                    headers.insert(name, val);
                }
            }
        }
        if let Some(extra_headers) = &opts.extra_headers {
            for (k, v) in extra_headers {
                if let (Ok(name), Ok(val)) = (
                    k.parse::<reqwest::header::HeaderName>(),
                    v.parse::<reqwest::header::HeaderValue>(),
                ) {
                    headers.insert(name, val);
                }
            }
        }

        let response = match crate::retry::retry_request(|| {
            client
                .post(&url)
                .bearer_auth(&api_key)
                .headers(headers.clone())
                .json(&body)
                .send()
        })
        .await
        {
            Ok(resp) => resp,
            Err(e) => {
                emit_error(&mut output, &mut tx, e.to_string());
                return;
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            emit_error(
                &mut output,
                &mut tx,
                format!("HTTP {}: {}", status, body_text),
            );
            return;
        }

        tx.push(AssistantMessageEvent::Start {
            partial: output.clone(),
        });

        let service_tier_str = opts.service_tier.clone();
        let service_tier = service_tier_str.as_deref();

        let sse_events = match sse::collect_sse_events(response, SseStop::RawMarker("[DONE]")).await
        {
            Ok(events) => events,
            Err(e) => {
                emit_error(&mut output, &mut tx, e.to_string());
                return;
            }
        };

        if let Err(e) =
            process_sse_events(sse_events, &mut output, &mut tx, &model, service_tier).await
        {
            emit_error(&mut output, &mut tx, e.to_string());
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

/// Set error state on output and push an Error event.
fn emit_error(
    output: &mut AssistantMessage,
    tx: &mut crate::stream::AssistantMessageEventSender,
    msg: String,
) {
    output.stop_reason = StopReason::Error;
    output.error_message = Some(msg);
    tx.push(AssistantMessageEvent::Error {
        reason: StopReason::Error,
        error: output.clone(),
    });
}

/// Parse SSE text into a list of JSON events.
///
/// Pure function over raw SSE text, exposed for property-based testing.
pub fn parse_sse_text(text: &str) -> Vec<Value> {
    let mut events = Vec::new();
    for raw_line in text.split('\n') {
        let line = raw_line.trim_end_matches('\r');
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" {
                break;
            }
            if let Ok(v) = serde_json::from_str::<Value>(data) {
                events.push(v);
            }
        }
    }
    events
}

// =============================================================================
// ApiProvider implementation
// =============================================================================

impl ApiProvider for OpenAIResponsesProvider {
    fn api(&self) -> &str {
        crate::types::known_api::OPENAI_RESPONSES
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
            Err(e) => return error_stream(model, e),
        };

        // Extract OpenAI-specific options from metadata
        let service_tier = base
            .metadata
            .as_ref()
            .and_then(|m| m.get("service_tier"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let reasoning_effort = base
            .metadata
            .as_ref()
            .and_then(|m| m.get("reasoning_effort"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let reasoning_summary = base
            .metadata
            .as_ref()
            .and_then(|m| m.get("reasoning_summary"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let opts = OpenAIRequestOptions {
            temperature: base.temperature,
            max_tokens: base.max_tokens,
            session_id: base.session_id.clone(),
            cache_retention: base.cache_retention.clone(),
            service_tier,
            reasoning_effort,
            reasoning_summary,
            extra_headers: base.headers.clone(),
        };

        stream_openai_responses(
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
            Err(e) => return error_stream(model, e),
        };

        // Resolve reasoning effort with xhigh clamping
        let reasoning_effort = opts.reasoning.as_ref().map(|level| {
            let effort = match level {
                ThinkingLevel::Minimal => "minimal",
                ThinkingLevel::Low => "low",
                ThinkingLevel::Medium => "medium",
                ThinkingLevel::High => "high",
                ThinkingLevel::XHigh => "xhigh",
            };
            clamp_reasoning_effort(effort, model).to_string()
        });

        let service_tier = base
            .metadata
            .as_ref()
            .and_then(|m| m.get("service_tier"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let request_opts = OpenAIRequestOptions {
            temperature: base.temperature,
            max_tokens: base.max_tokens,
            session_id: base.session_id.clone(),
            cache_retention: base.cache_retention.clone(),
            service_tier,
            reasoning_effort,
            reasoning_summary: None,
            extra_headers: base.headers.clone(),
        };

        stream_openai_responses(
            self.client.clone(),
            model.clone(),
            context.clone(),
            api_key,
            request_opts,
        )
    }
}
