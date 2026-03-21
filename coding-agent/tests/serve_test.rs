//! Integration tests for the JSON-RPC serve mode.
//!
//! These tests spawn the `coding-agent serve` binary, send JSON-RPC messages
//! on stdin, and verify responses/notifications on stdout.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::{json, Value};

/// Spawn `coding-agent serve` and return a handle with stdin/stdout access.
struct ServeProcess {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    reader: BufReader<std::process::ChildStdout>,
}

impl ServeProcess {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_coding-agent"))
            .args(["serve", "--cwd", "."])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn coding-agent serve");

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let reader = BufReader::new(stdout);

        ServeProcess {
            child,
            stdin,
            reader,
        }
    }

    /// Send a JSON-RPC request and return the parsed response.
    fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        writeln!(self.stdin, "{}", req).expect("write to stdin");
        self.stdin.flush().expect("flush stdin");

        let mut line = String::new();
        self.reader.read_line(&mut line).expect("read from stdout");
        serde_json::from_str(&line).expect("parse JSON response")
    }

    /// Send a JSON-RPC notification (no response expected).
    fn notify(&mut self, method: &str, params: Value) {
        let req = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        writeln!(self.stdin, "{}", req).expect("write to stdin");
        self.stdin.flush().expect("flush stdin");
    }

    /// Read the next line from stdout (for notifications).
    fn read_line(&mut self) -> Value {
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("read from stdout");
        serde_json::from_str(&line).expect("parse JSON notification")
    }

    fn shutdown(mut self) {
        let _ = self.request(999, "shutdown", json!({}));
        let _ = self
            .child
            .wait_timeout(Duration::from_secs(5))
            .ok()
            .flatten();
        let _ = self.child.kill();
    }
}

trait ChildExt {
    fn wait_timeout(&mut self, dur: Duration) -> std::io::Result<Option<std::process::ExitStatus>>;
}

impl ChildExt for std::process::Child {
    fn wait_timeout(&mut self, dur: Duration) -> std::io::Result<Option<std::process::ExitStatus>> {
        let start = std::time::Instant::now();
        loop {
            match self.try_wait()? {
                Some(status) => return Ok(Some(status)),
                None if start.elapsed() >= dur => return Ok(None),
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }
}

#[test]
fn test_initialize_returns_capabilities() {
    let mut proc = ServeProcess::spawn();

    let resp = proc.request(1, "initialize", json!({}));
    assert_eq!(resp["id"], 1);
    assert!(resp["result"]["capabilities"].is_object());
    assert!(resp["error"].is_null());

    proc.shutdown();
}

#[test]
fn test_initialized_notification_accepted() {
    let mut proc = ServeProcess::spawn();

    // Should not produce a response (it's a notification)
    proc.notify("initialized", json!({}));

    // Verify the process is still alive by sending a real request
    let resp = proc.request(1, "initialize", json!({}));
    assert_eq!(resp["id"], 1);
    assert!(resp["error"].is_null());

    proc.shutdown();
}

#[test]
fn test_session_status_starts_idle() {
    let mut proc = ServeProcess::spawn();

    let _ = proc.request(1, "initialize", json!({}));
    proc.notify("initialized", json!({}));

    let resp = proc.request(2, "session/status", json!({}));
    assert_eq!(resp["result"]["type"], "idle");

    proc.shutdown();
}

#[test]
fn test_session_messages_empty_initially() {
    let mut proc = ServeProcess::spawn();

    let _ = proc.request(1, "initialize", json!({}));

    let resp = proc.request(2, "session/messages", json!({}));
    assert!(resp["result"].is_array());
    assert_eq!(resp["result"].as_array().unwrap().len(), 0);

    proc.shutdown();
}

#[test]
fn test_session_abort_when_idle() {
    let mut proc = ServeProcess::spawn();

    let _ = proc.request(1, "initialize", json!({}));

    let resp = proc.request(2, "session/abort", json!({}));
    assert_eq!(resp["result"]["success"], true);

    proc.shutdown();
}

#[test]
fn test_unknown_method_returns_error() {
    let mut proc = ServeProcess::spawn();

    let resp = proc.request(1, "nonexistent/method", json!({}));
    assert!(resp["error"].is_object());
    assert_eq!(resp["error"]["code"], -32601); // METHOD_NOT_FOUND

    proc.shutdown();
}

#[test]
fn test_shutdown_exits_process() {
    let mut proc = ServeProcess::spawn();

    let resp = proc.request(1, "shutdown", json!({}));
    assert!(resp["error"].is_null());

    // Process should exit within a few seconds
    let status = proc
        .child
        .wait_timeout(Duration::from_secs(5))
        .expect("wait_timeout failed");
    assert!(
        status.is_some(),
        "Process should have exited after shutdown"
    );
}

#[test]
fn test_stdin_close_exits_process() {
    let mut proc = ServeProcess::spawn();

    // Close stdin by dropping it
    drop(proc.stdin);

    // Process should exit within a few seconds
    let status = proc
        .child
        .wait_timeout(Duration::from_secs(5))
        .expect("wait_timeout failed");
    assert!(
        status.is_some(),
        "Process should have exited after stdin close"
    );
}

#[test]
fn test_parse_error_returns_error_response() {
    let mut proc = ServeProcess::spawn();

    // Send malformed JSON
    writeln!(proc.stdin, "not json at all").expect("write");
    proc.stdin.flush().expect("flush");

    let resp = proc.read_line();
    assert!(resp["error"].is_object());
    assert_eq!(resp["error"]["code"], -32700); // PARSE_ERROR

    proc.shutdown();
}
