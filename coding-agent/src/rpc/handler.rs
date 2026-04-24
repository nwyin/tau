//! Request handler: dispatches JSON-RPC methods to agent operations.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use agent::types::{AgentEvent, AgentMessage};
use agent::Agent;
use ai::types::{ContentBlock, Message};
use serde_json::{json, Value};
use tokio::task::JoinHandle;

use super::transport::StdoutWriter;
use super::types::*;

// ---------------------------------------------------------------------------
// Session status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum SessionStatus {
    Idle,
    Busy,
    Error(String),
}

impl SessionStatus {
    pub fn as_str(&self) -> &str {
        match self {
            SessionStatus::Idle => "idle",
            SessionStatus::Busy => "busy",
            SessionStatus::Error(_) => "error",
        }
    }
}

// ---------------------------------------------------------------------------
// Server state
// ---------------------------------------------------------------------------

pub struct ServerState {
    pub agent: Agent,
    pub status: Mutex<SessionStatus>,
    pub writer: StdoutWriter,
    pub cumulative_usage: Arc<Mutex<UsageReport>>,
    pub agent_task: Mutex<Option<JoinHandle<()>>>,
    pub shutdown: AtomicBool,
}

// ---------------------------------------------------------------------------
// Top-level dispatch
// ---------------------------------------------------------------------------

pub async fn handle_request(state: &Arc<ServerState>, req: JsonRpcRequest) {
    // Notifications (no id) — don't send a response
    if req.id.is_none() {
        match req.method.as_str() {
            "initialized" => { /* handshake ack, no-op */ }
            _ => {
                eprintln!("[serve] unknown notification: {}", req.method);
            }
        }
        return;
    }

    let id = req.id.unwrap();
    let result = match req.method.as_str() {
        "initialize" => handle_initialize(),
        "session/send" => handle_session_send(state, req.params).await,
        "session/status" => handle_session_status(state),
        "session/messages" => handle_session_messages(state, req.params),
        "session/abort" => handle_session_abort(state),
        "shutdown" => handle_shutdown(state),
        _ => Err(JsonRpcError::new(
            METHOD_NOT_FOUND,
            format!("Method '{}' not found", req.method),
        )),
    };

    let resp = match result {
        Ok(value) => JsonRpcResponse::success(id, value),
        Err(err) => JsonRpcResponse::error(id, err),
    };
    state.writer.write_response(&resp);
}

// ---------------------------------------------------------------------------
// Handler implementations
// ---------------------------------------------------------------------------

fn handle_initialize() -> Result<Value, JsonRpcError> {
    Ok(json!({ "capabilities": {} }))
}

async fn handle_session_send(
    state: &Arc<ServerState>,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let params: SessionSendParams = params
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| JsonRpcError::new(INVALID_PARAMS, e.to_string()))?
        .ok_or_else(|| JsonRpcError::new(INVALID_PARAMS, "Missing params"))?;

    // Reject if already busy
    {
        let status = state.status.lock().unwrap();
        if *status == SessionStatus::Busy {
            return Err(JsonRpcError::new(SESSION_BUSY, "Session is busy"));
        }
    }

    // Apply optional overrides
    if let Some(system) = params.system {
        state.agent.set_system_prompt(system);
    }
    if let Some(model_id) = params.model {
        if let Some(model) = ai::models::find_model(&model_id) {
            state.agent.set_model((*model).clone());
        } else {
            eprintln!("[serve] unknown model '{}', keeping current", model_id);
        }
    }

    // Set status to busy before spawning (prevents race)
    *state.status.lock().unwrap() = SessionStatus::Busy;

    // Emit busy notification
    emit_status_notification(&state.writer, "busy", None, None, None);

    // Spawn agent loop in background
    let state_clone = Arc::clone(state);
    let prompt = params.prompt;
    let usage_before = { state.cumulative_usage.lock().unwrap().clone() };

    let handle = tokio::spawn(async move {
        let result = state_clone.agent.prompt(prompt).await;

        // Snapshot per-send usage and latest assistant output.
        let usage = {
            state_clone
                .cumulative_usage
                .lock()
                .unwrap()
                .saturating_delta_since(&usage_before)
        };
        let output = latest_assistant_output(&state_clone.agent);

        // Update status based on result
        let (new_status, error) = match result {
            Ok(_) => (SessionStatus::Idle, None),
            Err(e) => {
                eprintln!("[serve] agent error: {}", e);
                let message = e.to_string();
                (SessionStatus::Error(message.clone()), Some(message))
            }
        };

        let status_str = new_status.as_str().to_string();
        *state_clone.status.lock().unwrap() = new_status;

        // Emit idle/error notification with per-send result data.
        emit_status_notification(
            &state_clone.writer,
            &status_str,
            Some(usage),
            Some(output),
            error,
        );
    });

    *state.agent_task.lock().unwrap() = Some(handle);

    Ok(json!({}))
}

fn handle_session_status(state: &Arc<ServerState>) -> Result<Value, JsonRpcError> {
    let status = state.status.lock().unwrap();
    Ok(json!(SessionStatusResult {
        status_type: status.as_str().to_string(),
    }))
}

fn handle_session_messages(
    state: &Arc<ServerState>,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let limit = params
        .and_then(|p| serde_json::from_value::<SessionMessagesParams>(p).ok())
        .and_then(|p| p.limit);

    let entries: Vec<SessionMessageEntry> = state.agent.with_state(|s| {
        let messages = &s.messages;
        let iter: Box<dyn Iterator<Item = &AgentMessage>> = match limit {
            Some(n) => {
                let start = messages.len().saturating_sub(n);
                Box::new(messages[start..].iter())
            }
            None => Box::new(messages.iter()),
        };
        iter.filter_map(agent_message_to_entry).collect()
    });

    serde_json::to_value(entries).map_err(|e| JsonRpcError::new(INTERNAL_ERROR, e.to_string()))
}

fn handle_session_abort(state: &Arc<ServerState>) -> Result<Value, JsonRpcError> {
    state.agent.abort();
    Ok(json!({ "success": true }))
}

fn handle_shutdown(state: &Arc<ServerState>) -> Result<Value, JsonRpcError> {
    state.shutdown.store(true, Ordering::SeqCst);
    state.agent.abort();
    Ok(json!({}))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn emit_status_notification(
    writer: &StdoutWriter,
    status: &str,
    usage: Option<UsageReport>,
    output: Option<String>,
    error: Option<String>,
) {
    let notif = JsonRpcNotification::new(
        "session.status",
        json!(SessionStatusNotification {
            status: SessionStatusResult {
                status_type: status.to_string(),
            },
            usage,
            output,
            error,
        }),
    );
    writer.write_notification(&notif);
}

fn latest_assistant_output(agent: &Agent) -> String {
    agent.with_state(|state| assistant_output_from_messages(&state.messages))
}

fn assistant_output_from_messages(messages: &[AgentMessage]) -> String {
    messages
        .iter()
        .rev()
        .find_map(|msg| match msg {
            AgentMessage::Llm(Message::Assistant(am)) => {
                let text = am
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");
                Some(text)
            }
            _ => None,
        })
        .unwrap_or_default()
}

fn agent_message_to_entry(msg: &AgentMessage) -> Option<SessionMessageEntry> {
    match msg {
        AgentMessage::Llm(Message::User(um)) => Some(SessionMessageEntry {
            role: "user".to_string(),
            content: serde_json::to_value(&um.content).unwrap_or(Value::Null),
            metadata: None,
        }),
        AgentMessage::Llm(Message::Assistant(am)) => Some(SessionMessageEntry {
            role: "assistant".to_string(),
            content: serde_json::to_value(&am.content).unwrap_or(Value::Null),
            metadata: Some(json!({
                "model": am.model,
                "usage": {
                    "input_tokens": am.usage.input,
                    "output_tokens": am.usage.output,
                },
                "stop_reason": format!("{:?}", am.stop_reason),
            })),
        }),
        AgentMessage::Llm(Message::ToolResult(tr)) => Some(SessionMessageEntry {
            role: "tool_result".to_string(),
            content: serde_json::to_value(&tr.content).unwrap_or(Value::Null),
            metadata: None,
        }),
        AgentMessage::Custom { .. } => None,
    }
}

/// Create the event subscriber that tracks cumulative token usage.
pub fn usage_tracking_subscriber(
    usage: Arc<Mutex<UsageReport>>,
) -> impl Fn(&AgentEvent) + Send + Sync + 'static {
    move |event| match event {
        AgentEvent::TurnEnd {
            message: AgentMessage::Llm(Message::Assistant(am)),
            ..
        } => {
            let mut u = usage.lock().unwrap();
            u.input_tokens += am.usage.input;
            u.output_tokens += am.usage.output;
        }
        AgentEvent::ToolExecutionEnd { .. } => {
            usage.lock().unwrap().tool_calls += 1;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use agent::types::AgentToolResult;
    use ai::types::{AssistantMessage, StopReason, Usage, UserBlock};

    fn assistant_message(text: &str, input_tokens: u64, output_tokens: u64) -> AgentMessage {
        let mut message = AssistantMessage::zero_usage(
            "test-api",
            "test-provider",
            "test-model",
            StopReason::Stop,
        );
        message.content = vec![ContentBlock::Text {
            text: text.to_string(),
            text_signature: None,
        }];
        message.usage = Usage {
            input: input_tokens,
            output: output_tokens,
            ..Usage::default()
        };
        AgentMessage::Llm(Message::Assistant(message))
    }

    #[test]
    fn assistant_output_uses_latest_assistant_text_blocks() {
        let mut first = AssistantMessage::zero_usage(
            "test-api",
            "test-provider",
            "test-model",
            StopReason::Stop,
        );
        first.content = vec![ContentBlock::Text {
            text: "old".to_string(),
            text_signature: None,
        }];

        let mut latest = AssistantMessage::zero_usage(
            "test-api",
            "test-provider",
            "test-model",
            StopReason::Stop,
        );
        latest.content = vec![
            ContentBlock::Text {
                text: "new".to_string(),
                text_signature: None,
            },
            ContentBlock::ToolCall {
                id: "call-1".to_string(),
                name: "fake".to_string(),
                arguments: Default::default(),
                thought_signature: None,
            },
            ContentBlock::Text {
                text: " output".to_string(),
                text_signature: None,
            },
        ];

        let messages = vec![
            AgentMessage::Llm(Message::Assistant(first)),
            AgentMessage::Llm(Message::Assistant(latest)),
        ];

        assert_eq!(assistant_output_from_messages(&messages), "new output");
    }

    #[test]
    fn usage_tracking_subscriber_tracks_tokens_and_tool_calls() {
        let usage = Arc::new(Mutex::new(UsageReport::default()));
        let subscriber = usage_tracking_subscriber(Arc::clone(&usage));

        subscriber(&AgentEvent::TurnEnd {
            message: assistant_message("done", 11, 7),
            tool_results: Vec::new(),
        });
        subscriber(&AgentEvent::ToolExecutionEnd {
            tool_call_id: "call-1".to_string(),
            tool_name: "fake".to_string(),
            result: AgentToolResult {
                content: vec![UserBlock::Text {
                    text: "ok".to_string(),
                }],
                details: None,
            },
            is_error: false,
            thread_id: None,
            thread_alias: None,
        });

        assert_eq!(
            *usage.lock().unwrap(),
            UsageReport {
                input_tokens: 11,
                output_tokens: 7,
                tool_calls: 1,
            }
        );
    }
}
