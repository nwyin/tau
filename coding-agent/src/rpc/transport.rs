//! Stdin/stdout transport for JSON-RPC over newline-delimited JSON.

use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use super::types::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};

// ---------------------------------------------------------------------------
// StdinReader
// ---------------------------------------------------------------------------

/// Spawns a blocking thread that reads JSON-RPC requests from stdin.
/// Returns an async receiver of parsed requests (or raw lines on parse failure).
pub fn spawn_stdin_reader() -> mpsc::UnboundedReceiver<StdinMessage> {
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::task::spawn_blocking(move || {
        let stdin = io::stdin();
        let reader = io::BufReader::new(stdin.lock());
        for line in reader.lines() {
            match line {
                Ok(line) if line.trim().is_empty() => continue,
                Ok(line) => {
                    let msg = match serde_json::from_str::<JsonRpcRequest>(&line) {
                        Ok(req) => StdinMessage::Request(req),
                        Err(e) => StdinMessage::ParseError(e.to_string()),
                    };
                    if tx.send(msg).is_err() {
                        break; // receiver dropped
                    }
                }
                Err(_) => break, // stdin closed or error
            }
        }
    });

    rx
}

pub enum StdinMessage {
    Request(JsonRpcRequest),
    ParseError(String),
}

// ---------------------------------------------------------------------------
// StdoutWriter
// ---------------------------------------------------------------------------

/// Thread-safe writer for JSON-RPC responses and notifications to stdout.
#[derive(Clone)]
pub struct StdoutWriter {
    inner: Arc<Mutex<io::Stdout>>,
}

impl Default for StdoutWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl StdoutWriter {
    pub fn new() -> Self {
        StdoutWriter {
            inner: Arc::new(Mutex::new(io::stdout())),
        }
    }

    pub fn write_response(&self, resp: &JsonRpcResponse) {
        if let Ok(line) = serde_json::to_string(resp) {
            let mut out = self.inner.lock().unwrap();
            let _ = writeln!(out, "{}", line);
            let _ = out.flush();
        }
    }

    pub fn write_notification(&self, notif: &JsonRpcNotification) {
        if let Ok(line) = serde_json::to_string(notif) {
            let mut out = self.inner.lock().unwrap();
            let _ = writeln!(out, "{}", line);
            let _ = out.flush();
        }
    }
}
