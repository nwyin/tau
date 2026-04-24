//! JSON-RPC 2.0 types and per-method param/result structs.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 core types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}

// ---------------------------------------------------------------------------
// Standard error codes
// ---------------------------------------------------------------------------

pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;
pub const SESSION_BUSY: i32 = -32000;

// ---------------------------------------------------------------------------
// Convenience constructors
// ---------------------------------------------------------------------------

impl JsonRpcResponse {
    pub fn success(id: Value, result: Value) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Value, err: JsonRpcError) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(err),
        }
    }
}

impl JsonRpcError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        JsonRpcError {
            code,
            message: message.into(),
            data: None,
        }
    }
}

impl JsonRpcNotification {
    pub fn new(method: impl Into<String>, params: Value) -> Self {
        JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

// ---------------------------------------------------------------------------
// Per-method param/result types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SessionSendParams {
    pub prompt: String,
    pub system: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionStatusResult {
    #[serde(rename = "type")]
    pub status_type: String,
}

#[derive(Debug, Deserialize)]
pub struct SessionMessagesParams {
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct SessionMessageEntry {
    pub role: String,
    pub content: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UsageReport {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub tool_calls: u64,
}

impl UsageReport {
    pub fn saturating_delta_since(&self, before: &UsageReport) -> UsageReport {
        UsageReport {
            input_tokens: self.input_tokens.saturating_sub(before.input_tokens),
            output_tokens: self.output_tokens.saturating_sub(before.output_tokens),
            tool_calls: self.tool_calls.saturating_sub(before.tool_calls),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SessionStatusNotification {
    pub status: SessionStatusResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    #[test]
    fn usage_report_delta_is_per_send_and_saturating() {
        let before = UsageReport {
            input_tokens: 10,
            output_tokens: 4,
            tool_calls: 2,
        };
        let after = UsageReport {
            input_tokens: 17,
            output_tokens: 9,
            tool_calls: 5,
        };

        assert_eq!(
            after.saturating_delta_since(&before),
            UsageReport {
                input_tokens: 7,
                output_tokens: 5,
                tool_calls: 3,
            }
        );
        assert_eq!(
            before.saturating_delta_since(&after),
            UsageReport::default()
        );
    }

    #[test]
    fn session_status_notification_serializes_output_error_and_tool_calls() {
        let notification = SessionStatusNotification {
            status: SessionStatusResult {
                status_type: "idle".to_string(),
            },
            usage: Some(UsageReport {
                input_tokens: 11,
                output_tokens: 7,
                tool_calls: 1,
            }),
            output: Some("DONE".to_string()),
            error: None,
        };

        assert_eq!(
            serde_json::to_value(notification).unwrap(),
            json!({
                "status": {"type": "idle"},
                "usage": {
                    "input_tokens": 11,
                    "output_tokens": 7,
                    "tool_calls": 1,
                },
                "output": "DONE",
            })
        );
    }
}
