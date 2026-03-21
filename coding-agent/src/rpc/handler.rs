//! Request handler: dispatches JSON-RPC methods to agent operations.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use agent::types::{AgentEvent, AgentMessage};
use agent::Agent;
use ai::types::Message;
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
    emit_status_notification(&state.writer, "busy", None);

    // Spawn agent loop in background
    let state_clone = Arc::clone(state);
    let prompt = params.prompt;

    let handle = tokio::spawn(async move {
        let result = state_clone.agent.prompt(prompt).await;

        // Snapshot cumulative usage
        let usage = { state_clone.cumulative_usage.lock().unwrap().clone() };

        // Update status based on result
        let new_status = match result {
            Ok(_) => SessionStatus::Idle,
            Err(e) => {
                eprintln!("[serve] agent error: {}", e);
                SessionStatus::Error(e.to_string())
            }
        };

        let status_str = new_status.as_str().to_string();
        *state_clone.status.lock().unwrap() = new_status;

        // Emit idle/error notification with usage
        emit_status_notification(&state_clone.writer, &status_str, Some(usage));
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

fn emit_status_notification(writer: &StdoutWriter, status: &str, usage: Option<UsageReport>) {
    let notif = JsonRpcNotification::new(
        "session.status",
        json!(SessionStatusNotification {
            status: SessionStatusResult {
                status_type: status.to_string(),
            },
            usage,
        }),
    );
    writer.write_notification(&notif);
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
    move |event| {
        if let AgentEvent::TurnEnd {
            message: AgentMessage::Llm(Message::Assistant(am)),
            ..
        } = event
        {
            let mut u = usage.lock().unwrap();
            u.input_tokens += am.usage.input;
            u.output_tokens += am.usage.output;
        }
    }
}
