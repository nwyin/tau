//! OpenAI Chat Completions API provider implementation.
//!
//! Covers: OpenRouter (all models), direct OpenAI Chat Completions,
//! and any OpenAI-compatible endpoint (Groq, Together, Ollama, etc.).

use anyhow::Result;
use futures::StreamExt;
use serde_json::Value;

use crate::providers::openai_chat_shared::{
    build_chat_request_body, process_chat_sse_events, ChatRequestOptions,
};
use crate::providers::openai_responses_shared::clamp_reasoning_effort;
use crate::providers::ApiProvider;
use crate::stream::{assistant_message_event_stream, AssistantMessageEventStream};
use crate::types::{
    AssistantMessage, AssistantMessageEvent, Context, Cost, Model, SimpleStreamOptions, StopReason,
    StreamOptions, ThinkingLevel, Usage,
};

// =============================================================================
// Provider struct
// =============================================================================

pub struct OpenAIChatProvider {
    client: reqwest::Client,
}

impl OpenAIChatProvider {
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
            "openrouter" => "OPENROUTER_API_KEY",
            "groq" => "GROQ_API_KEY",
            "together" => "TOGETHER_API_KEY",
            "deepseek" => "DEEPSEEK_API_KEY",
            _ => "OPENAI_API_KEY",
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

impl Default for OpenAIChatProvider {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Core streaming function
// =============================================================================

fn stream_openai_chat(
    client: reqwest::Client,
    model: Model,
    context: Context,
    api_key: String,
    opts: ChatRequestOptions,
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

        let body = build_chat_request_body(&model, &context, &opts);
        let url = format!("{}/chat/completions", model.base_url);

        let mut req = client
            .post(&url)
            .bearer_auth(&api_key)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream");

        // OpenRouter-specific headers
        if model.base_url.contains("openrouter.ai") {
            req = req
                .header("HTTP-Referer", "https://github.com/nwyin/tau")
                .header("X-Title", "tau");
        }

        // Model-level headers
        if let Some(headers) = &model.headers {
            for (k, v) in headers {
                req = req.header(k, v);
            }
        }
        // Option-level headers
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

        // Emit Start event
        tx.push(AssistantMessageEvent::Start {
            partial: output.clone(),
        });

        // Collect SSE events
        let sse_events = match collect_sse_events(response).await {
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

        // Process events
        if let Err(e) = process_chat_sse_events(sse_events, &mut output, &mut tx, &model).await {
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

/// Collect SSE data lines from a streaming HTTP response.
///
/// Reads bytes, splits on newlines, and parses `data: {...}` lines.
/// Stops on `data: [DONE]`.
async fn collect_sse_events(response: reqwest::Response) -> Result<Vec<Value>> {
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
                        if data == "[DONE]" {
                            return Ok(events);
                        }
                        if let Ok(v) = serde_json::from_str::<Value>(data) {
                            events.push(v);
                        }
                    }
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
// ApiProvider implementation
// =============================================================================

impl ApiProvider for OpenAIChatProvider {
    fn api(&self) -> &str {
        crate::types::known_api::OPENAI_CHAT
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

        let reasoning_effort = base
            .metadata
            .as_ref()
            .and_then(|m| m.get("reasoning_effort"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let opts = ChatRequestOptions {
            temperature: base.temperature,
            max_tokens: base.max_tokens,
            reasoning_effort,
            extra_headers: base.headers.clone(),
        };

        stream_openai_chat(
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

        let request_opts = ChatRequestOptions {
            temperature: base.temperature,
            max_tokens: base.max_tokens,
            reasoning_effort,
            extra_headers: base.headers.clone(),
        };

        stream_openai_chat(
            self.client.clone(),
            model.clone(),
            context.clone(),
            api_key,
            request_opts,
        )
    }
}
