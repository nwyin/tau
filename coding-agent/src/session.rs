//! Session persistence — JSONL-based conversation history.
//!
//! Format:
//!   Line 1 (header): {"type":"session","version":1,"id":"<8-hex>","timestamp":"<iso8601>","cwd":"<abs>"}
//!   Subsequent lines: {"type":"message","timestamp":"<iso8601>","message":<AgentMessage JSON>}

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use agent::types::AgentMessage;
use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// SessionManager
// ---------------------------------------------------------------------------

pub struct SessionManager {
    pub session_dir: PathBuf,
}

pub struct SessionFile {
    pub id: String,
    pub path: PathBuf,
}

impl SessionManager {
    pub fn new(session_dir: PathBuf) -> Self {
        SessionManager { session_dir }
    }

    /// Create a new session file with a header line.
    pub fn create(&self, cwd: &Path) -> Result<SessionFile> {
        fs::create_dir_all(&self.session_dir).context("Failed to create session directory")?;

        let id = generate_session_id();
        let path = self.session_dir.join(format!("{}.jsonl", id));

        let header = serde_json::json!({
            "type": "session",
            "version": 1,
            "id": id,
            "timestamp": Utc::now().to_rfc3339(),
            "cwd": cwd.to_string_lossy()
        });

        let mut file = File::create(&path).context("Failed to create session file")?;
        writeln!(file, "{}", serde_json::to_string(&header)?)?;

        Ok(SessionFile { id, path })
    }

    /// Open an existing session for appending (does not read messages).
    pub fn open(&self, session_id: &str) -> Result<SessionFile> {
        let path = self.session_dir.join(format!("{}.jsonl", session_id));
        if !path.exists() {
            anyhow::bail!(
                "Session '{}' not found (looked at {})",
                session_id,
                path.display()
            );
        }
        Ok(SessionFile {
            id: session_id.to_string(),
            path,
        })
    }

    /// Parse a session file and return all messages, skipping malformed lines.
    pub fn load(&self, session_id: &str) -> Result<Vec<AgentMessage>> {
        let path = self.session_dir.join(format!("{}.jsonl", session_id));
        if !path.exists() {
            anyhow::bail!(
                "Session '{}' not found (looked at {})",
                session_id,
                path.display()
            );
        }

        let file = File::open(&path).context("Failed to open session file")?;
        let reader = BufReader::new(file);
        let mut messages = vec![];

        for (line_num, line_result) in reader.lines().enumerate() {
            let line = match line_result {
                Ok(l) => l,
                Err(e) => {
                    eprintln!(
                        "Warning: failed to read session line {}: {}",
                        line_num + 1,
                        e
                    );
                    continue;
                }
            };

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let value: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!(
                        "Warning: skipping malformed JSON at session line {}: {}",
                        line_num + 1,
                        e
                    );
                    continue;
                }
            };

            let line_type = value.get("type").and_then(Value::as_str).unwrap_or("");
            if line_type != "message" {
                continue;
            }

            let msg_value = value.get("message").cloned().unwrap_or(Value::Null);
            match serde_json::from_value::<AgentMessage>(msg_value) {
                Ok(m) => messages.push(m),
                Err(e) => {
                    eprintln!(
                        "Warning: skipping undeserializable message at session line {}: {}",
                        line_num + 1,
                        e
                    );
                }
            }
        }

        Ok(messages)
    }

    /// List sessions for a specific working directory, most recent first.
    /// Returns (id, timestamp, message_count) tuples.
    pub fn list_for_cwd(&self, cwd: &Path) -> Result<Vec<(String, String, usize)>> {
        if !self.session_dir.exists() {
            return Ok(vec![]);
        }

        let cwd_str = cwd.to_string_lossy().to_string();
        let mut sessions = vec![];

        for entry in fs::read_dir(&self.session_dir).context("Failed to read session directory")? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }

            // Read the header line to check cwd
            let file = match File::open(&path) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let mut reader = BufReader::new(file);
            let mut header_line = String::new();
            if reader.read_line(&mut header_line).is_err() {
                continue;
            }

            let header: Value = match serde_json::from_str(header_line.trim()) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let session_cwd = header.get("cwd").and_then(Value::as_str).unwrap_or("");
            if session_cwd != cwd_str {
                continue;
            }

            let id = header
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let timestamp = header
                .get("timestamp")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();

            // Count message lines
            let msg_count = reader
                .lines()
                .filter(|l| {
                    l.as_ref()
                        .ok()
                        .and_then(|line| serde_json::from_str::<Value>(line).ok())
                        .and_then(|v| {
                            v.get("type")
                                .and_then(Value::as_str)
                                .map(|t| t == "message")
                        })
                        .unwrap_or(false)
                })
                .count();

            let mtime = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

            sessions.push((mtime, id, timestamp, msg_count));
        }

        sessions.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(sessions
            .into_iter()
            .map(|(_, id, ts, count)| (id, ts, count))
            .collect())
    }

    /// Find the most recently modified session for a cwd, returning its ID.
    pub fn latest_for_cwd(&self, cwd: &Path) -> Result<Option<String>> {
        Ok(self
            .list_for_cwd(cwd)?
            .into_iter()
            .next()
            .map(|(id, _, _)| id))
    }

    /// Find the most recently modified session file, returning its ID.
    pub fn latest(&self) -> Result<Option<String>> {
        if !self.session_dir.exists() {
            return Ok(None);
        }

        let mut entries: Vec<(std::time::SystemTime, String)> = vec![];

        for entry in fs::read_dir(&self.session_dir).context("Failed to read session directory")? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        entries.push((mtime, stem.to_string()));
                    }
                }
            }
        }

        entries.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(entries.into_iter().next().map(|(_, id)| id))
    }
}

// ---------------------------------------------------------------------------
// SessionFile
// ---------------------------------------------------------------------------

impl SessionFile {
    /// Append a single message line to the session file.
    pub fn append(&self, message: &AgentMessage) -> Result<()> {
        let line = serde_json::json!({
            "type": "message",
            "timestamp": Utc::now().to_rfc3339(),
            "message": message
        });

        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.path)
            .with_context(|| {
                format!(
                    "Failed to open session file for append: {}",
                    self.path.display()
                )
            })?;

        writeln!(file, "{}", serde_json::to_string(&line)?)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate an 8-hex-char session ID from a UUID v4.
fn generate_session_id() -> String {
    let id = Uuid::new_v4().to_string().replace('-', "");
    id[..8].to_string()
}
