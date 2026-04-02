//! Shared SSE (Server-Sent Events) stream collection.
//!
//! All three provider implementations parse SSE the same way — read bytes,
//! split on newlines, extract `data: ` lines, and parse JSON. The only
//! difference is how they detect the end of the stream:
//!
//! - Anthropic: JSON event with `"type": "message_stop"`
//! - OpenAI: raw `data: [DONE]` sentinel

use anyhow::Result;
use futures::StreamExt;
use serde_json::Value;

/// How to detect the end of an SSE stream.
pub enum SseStop {
    /// Stop when the raw data line equals this string (e.g. `[DONE]`).
    /// The sentinel is NOT included in the returned events.
    RawMarker(&'static str),
    /// Stop when a parsed JSON event's `"type"` field matches this value.
    /// The stop event IS included in the returned events.
    JsonType(&'static str),
}

/// Collect SSE data events from a streaming HTTP response.
///
/// Reads bytes incrementally, splits on newlines, parses `data: {...}` lines
/// into JSON, and stops according to the given [`SseStop`] condition.
pub async fn collect_sse_events(
    response: reqwest::Response,
    stop: SseStop,
) -> Result<Vec<Value>> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut events: Vec<Value> = Vec::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        loop {
            let Some(pos) = buffer.find('\n') else {
                break;
            };
            let line = buffer[..pos].trim_end_matches('\r').to_string();
            buffer = buffer[pos + 1..].to_string();

            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };

            // Check raw-text stop condition before parsing
            if let SseStop::RawMarker(marker) = &stop {
                if data == *marker {
                    return Ok(events);
                }
            }

            match serde_json::from_str::<Value>(data) {
                Ok(v) => {
                    // Check JSON-type stop condition after parsing
                    if let SseStop::JsonType(type_val) = &stop {
                        let is_stop =
                            v.get("type").and_then(|t| t.as_str()) == Some(*type_val);
                        events.push(v);
                        if is_stop {
                            return Ok(events);
                        }
                    } else {
                        events.push(v);
                    }
                }
                Err(_) => {
                    // Malformed JSON — skip gracefully
                }
            }
        }
    }

    // Stream ended without the stop condition
    if events.is_empty() {
        return Err(anyhow::anyhow!("Empty response stream"));
    }

    Ok(events)
}
