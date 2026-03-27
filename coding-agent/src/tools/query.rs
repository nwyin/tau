//! Query tool: single-shot LLM call without tools.
//!
//! For quick classification, decision, or extraction tasks where a full
//! agent loop with tools is unnecessary.

use std::sync::Arc;

use agent::orchestrator::OrchestratorState;
use agent::thread::{Episode, ThreadOutcome};
use agent::types::{AgentTool, AgentToolResult, BoxFuture, GetApiKeyFn, ToolUpdateFn};
use ai::types::{Model, UserBlock};
use futures::StreamExt;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

pub struct QueryTool {
    orchestrator: Arc<OrchestratorState>,
    get_api_key: Option<GetApiKeyFn>,
    default_model: Model,
}

impl QueryTool {
    pub fn new(
        orchestrator: Arc<OrchestratorState>,
        get_api_key: Option<GetApiKeyFn>,
        default_model: Model,
    ) -> Self {
        Self {
            orchestrator,
            get_api_key,
            default_model,
        }
    }

    pub fn arc(
        orchestrator: Arc<OrchestratorState>,
        get_api_key: Option<GetApiKeyFn>,
        default_model: Model,
    ) -> Arc<dyn AgentTool> {
        Arc::new(Self::new(orchestrator, get_api_key, default_model))
    }
}

impl AgentTool for QueryTool {
    fn name(&self) -> &str {
        "query"
    }

    fn label(&self) -> &str {
        "Query"
    }

    fn description(&self) -> &str {
        "Single-shot LLM call without tools. Use for quick classification, decisions, \
         summarization, or extraction tasks that don't need tool access. Faster and cheaper \
         than spawning a thread."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "alias": {
                        "type": "string",
                        "description": "Optional name for this query, so its result can be referenced by threads via the episodes parameter."
                    },
                    "prompt": {
                        "type": "string",
                        "description": "The question or instruction for the LLM."
                    },
                    "model": {
                        "type": "string",
                        "description": "Model override (e.g. 'claude-haiku-4-5' for cheap queries)."
                    }
                },
                "required": ["prompt"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<CancellationToken>,
        _on_update: Option<ToolUpdateFn>,
    ) -> BoxFuture<anyhow::Result<AgentToolResult>> {
        let orchestrator = self.orchestrator.clone();
        let get_api_key = self.get_api_key.clone();
        let default_model = self.default_model.clone();

        Box::pin(async move {
            let prompt = params
                .get("prompt")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing 'prompt' parameter"))?
                .to_string();
            let alias = params
                .get("alias")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| format!("query-{}", orchestrator.next_thread_id()));
            let model_override = params
                .get("model")
                .and_then(|v| v.as_str())
                .map(String::from);

            // Resolve model
            let model = if let Some(ref model_id) = model_override {
                ai::models::find_model(model_id)
                    .map(|m| (*m).clone())
                    .unwrap_or(default_model)
            } else {
                default_model
            };

            // Resolve API key
            let api_key = if let Some(ref get_key) = get_api_key {
                (get_key)(model.provider.clone()).await
            } else {
                None
            };

            // Build context: just system + user message
            let context = ai::types::Context {
                system_prompt: Some(
                    "You are a helpful assistant. Answer concisely and directly.".to_string(),
                ),
                messages: vec![ai::types::Message::User(ai::types::UserMessage::new(
                    &prompt,
                ))],
                tools: None,
            };

            let opts = ai::types::SimpleStreamOptions {
                reasoning: None,
                thinking_budgets: None,
                base: ai::types::StreamOptions {
                    api_key,
                    ..Default::default()
                },
            };

            let start = std::time::Instant::now();

            // Stream the response
            let event_stream = ai::stream_simple(&model, &context, Some(&opts))?;
            let mut pinned = Box::pin(event_stream);
            let mut response_text = String::new();

            while let Some(event) = pinned.next().await {
                match event {
                    ai::types::AssistantMessageEvent::Done { message, .. } => {
                        for block in &message.content {
                            if let ai::types::ContentBlock::Text { text, .. } = block {
                                response_text.push_str(text);
                            }
                        }
                        break;
                    }
                    ai::types::AssistantMessageEvent::Error { error, .. } => {
                        if let Some(err) = &error.error_message {
                            return Ok(AgentToolResult {
                                content: vec![UserBlock::Text {
                                    text: format!("Query error: {}", err),
                                }],
                                details: None,
                            });
                        }
                        break;
                    }
                    _ => {}
                }
            }

            let duration_ms = start.elapsed().as_millis() as u64;

            // Record as lightweight episode
            let thread_id = orchestrator.next_thread_id();
            orchestrator.get_or_create_thread(&alias, "");
            let episode = Episode {
                thread_id: thread_id.clone(),
                alias: alias.clone(),
                task: prompt.clone(),
                outcome: ThreadOutcome::Completed {
                    result: response_text.clone(),
                    evidence: vec![],
                },
                full_trace: format!(
                    "--- Query: {} ---\nPROMPT: {}\nOUTPUT: {}\n",
                    alias, prompt, response_text
                ),
                compact_trace: format!(
                    "--- Query: {} ---\nPROMPT: {}\nOUTPUT: {}\n",
                    alias, prompt, response_text
                ),
                duration_ms,
                turn_count: 1,
            };
            orchestrator.record_episode(episode, vec![]);

            Ok(AgentToolResult {
                content: vec![UserBlock::Text {
                    text: response_text,
                }],
                details: Some(json!({
                    "thread_id": thread_id,
                    "alias": alias,
                    "duration_ms": duration_ms,
                })),
            })
        })
    }
}
