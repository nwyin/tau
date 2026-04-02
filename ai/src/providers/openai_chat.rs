//! OpenAI Chat Completions API provider implementation.
//!
//! Covers: OpenRouter (all models), direct OpenAI Chat Completions,
//! and any OpenAI-compatible endpoint (Groq, Together, Ollama, etc.).

use anyhow::Result;

use crate::providers::openai_chat_shared::{
    build_chat_request_body, process_chat_sse_events, ChatRequestOptions,
};
use crate::providers::openai_responses_shared::clamp_reasoning_effort;
use crate::providers::sse::{self, SseStop};
use crate::providers::ApiProvider;
use crate::stream::{assistant_message_event_stream, error_stream, AssistantMessageEventStream};
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

        let body = build_chat_request_body(&model, &context, &opts);
        let url = format!("{}/chat/completions", model.base_url);

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());
        headers.insert("Accept", "text/event-stream".parse().unwrap());

        // OpenRouter-specific headers
        if model.base_url.contains("openrouter.ai") {
            headers.insert(
                "HTTP-Referer",
                "https://github.com/nwyin/tau".parse().unwrap(),
            );
            headers.insert("X-Title", "tau".parse().unwrap());
        }

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
            emit_error(&mut output, &mut tx, format!("HTTP {}: {}", status, body_text));
            return;
        }

        tx.push(AssistantMessageEvent::Start {
            partial: output.clone(),
        });

        let sse_events =
            match sse::collect_sse_events(response, SseStop::RawMarker("[DONE]")).await {
                Ok(events) => events,
                Err(e) => {
                    emit_error(&mut output, &mut tx, e.to_string());
                    return;
                }
            };

        if let Err(e) = process_chat_sse_events(sse_events, &mut output, &mut tx, &model).await {
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
            Err(e) => return error_stream(model, e),
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
            Err(e) => return error_stream(model, e),
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
