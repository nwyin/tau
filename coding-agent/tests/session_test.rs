//! Tests for session persistence (JSONL conversation history).

use std::io::Write;

use agent::types::AgentMessage;
use ai::types::{
    AssistantMessage, ContentBlock, Message, StopReason, ToolResultMessage, Usage, UserMessage,
};
use coding_agent::session::SessionManager;
use tempfile::tempdir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn user_msg(text: &str) -> AgentMessage {
    AgentMessage::Llm(Message::User(UserMessage::new(text)))
}

fn assistant_msg(text: &str) -> AgentMessage {
    AgentMessage::Llm(Message::Assistant(AssistantMessage {
        role: "assistant".into(),
        content: vec![ContentBlock::Text {
            text: text.into(),
            text_signature: None,
        }],
        api: "openai-responses".into(),
        provider: "openai".into(),
        model: "gpt-4o-mini".into(),
        usage: Usage::default(),
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 1_000_000,
    }))
}

fn tool_result_msg(tool_call_id: &str, content: &str) -> AgentMessage {
    AgentMessage::Llm(Message::ToolResult(ToolResultMessage {
        role: "toolResult".into(),
        tool_call_id: tool_call_id.into(),
        tool_name: "bash".into(),
        content: vec![ai::types::UserBlock::Text {
            text: content.into(),
        }],
        details: None,
        is_error: false,
        timestamp: 1_000_000,
    }))
}

fn custom_msg(role: &str, data: serde_json::Value) -> AgentMessage {
    AgentMessage::Custom {
        role: role.to_string(),
        data,
    }
}

// ---------------------------------------------------------------------------
// INV-4: Session header contains valid ID (8 hex chars) and ISO-8601 timestamp
// ---------------------------------------------------------------------------

#[test]
fn test_session_header_valid_id_and_timestamp() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path().join("sessions"));

    let sf = mgr.create(dir.path()).unwrap();

    // ID must be exactly 8 hex chars
    assert_eq!(sf.id.len(), 8, "session ID must be 8 chars");
    assert!(
        sf.id.chars().all(|c| c.is_ascii_hexdigit()),
        "session ID must be hex: got '{}'",
        sf.id
    );

    // Verify file exists and first line is valid JSON with expected fields
    let content = std::fs::read_to_string(&sf.path).unwrap();
    let first_line = content.lines().next().unwrap();
    let header: serde_json::Value = serde_json::from_str(first_line).unwrap();

    assert_eq!(header["type"], "session");
    assert_eq!(header["version"], 1);
    assert_eq!(header["id"].as_str().unwrap(), sf.id);

    // Timestamp must be parseable as ISO-8601
    let ts = header["timestamp"].as_str().unwrap();
    chrono::DateTime::parse_from_rfc3339(ts)
        .unwrap_or_else(|e| panic!("timestamp '{}' is not valid ISO-8601: {}", ts, e));
}

// ---------------------------------------------------------------------------
// INV-1: Written session file is valid JSONL (each line parses independently)
// ---------------------------------------------------------------------------

#[test]
fn test_session_file_is_valid_jsonl() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path().join("sessions"));

    let sf = mgr.create(dir.path()).unwrap();
    sf.append(&user_msg("hello")).unwrap();
    sf.append(&assistant_msg("hi there")).unwrap();

    let content = std::fs::read_to_string(&sf.path).unwrap();
    for (i, line) in content.lines().enumerate() {
        let result = serde_json::from_str::<serde_json::Value>(line);
        assert!(
            result.is_ok(),
            "line {} is not valid JSON: {:?}",
            i + 1,
            result
        );
    }
}

// ---------------------------------------------------------------------------
// INV-2: Load after save round-trips all messages faithfully
// ---------------------------------------------------------------------------

#[test]
fn test_round_trip_three_messages() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path().join("sessions"));

    let sf = mgr.create(dir.path()).unwrap();
    let msgs = vec![
        user_msg("what is 2+2"),
        assistant_msg("4"),
        tool_result_msg("call_1", "output text"),
    ];
    for m in &msgs {
        sf.append(m).unwrap();
    }

    let loaded = mgr.load(&sf.id).unwrap();
    assert_eq!(loaded.len(), 3, "expected 3 messages, got {}", loaded.len());

    // Verify content round-trips
    for (i, (orig, loaded)) in msgs.iter().zip(loaded.iter()).enumerate() {
        let orig_json = serde_json::to_string(orig).unwrap();
        let loaded_json = serde_json::to_string(loaded).unwrap();
        assert_eq!(
            orig_json, loaded_json,
            "message {} didn't round-trip: orig={} loaded={}",
            i, orig_json, loaded_json
        );
    }
}

#[test]
fn test_round_trip_custom_message() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path().join("sessions"));

    let sf = mgr.create(dir.path()).unwrap();
    let msg = custom_msg("system", serde_json::json!({"key": "value", "num": 42}));
    sf.append(&msg).unwrap();

    let loaded = mgr.load(&sf.id).unwrap();
    assert_eq!(loaded.len(), 1);

    let loaded_json = serde_json::to_string(&loaded[0]).unwrap();
    let orig_json = serde_json::to_string(&msg).unwrap();
    assert_eq!(orig_json, loaded_json, "custom message didn't round-trip");
}

// ---------------------------------------------------------------------------
// Resume: save 3 messages, resume, add 2 more → file has header + 5 lines
// ---------------------------------------------------------------------------

#[test]
fn test_resume_adds_to_existing_session() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path().join("sessions"));

    // First "run": create session, save 3 messages
    let sf = mgr.create(dir.path()).unwrap();
    let id = sf.id.clone();
    sf.append(&user_msg("msg1")).unwrap();
    sf.append(&assistant_msg("resp1")).unwrap();
    sf.append(&user_msg("msg2")).unwrap();

    // Second "run": open existing session, add 2 more
    let sf2 = mgr.open(&id).unwrap();
    sf2.append(&assistant_msg("resp2")).unwrap();
    sf2.append(&user_msg("msg3")).unwrap();

    // Total lines: 1 header + 5 messages = 6 lines
    let content = std::fs::read_to_string(&sf.path).unwrap();
    let line_count = content.lines().count();
    assert_eq!(line_count, 6, "expected 6 lines (1 header + 5 messages)");

    // Load should return all 5 messages
    let loaded = mgr.load(&id).unwrap();
    assert_eq!(loaded.len(), 5);
}

// ---------------------------------------------------------------------------
// INV-3: Malformed lines are skipped without error
// ---------------------------------------------------------------------------

#[test]
fn test_malformed_lines_skipped_gracefully() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path().join("sessions"));

    let sf = mgr.create(dir.path()).unwrap();
    sf.append(&user_msg("good message 1")).unwrap();

    // Inject corrupt data directly into the file
    {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&sf.path)
            .unwrap();
        writeln!(f, "{{not valid json at all %%%{{}}").unwrap();
        writeln!(f, "random bytes here no json").unwrap();
    }

    sf.append(&assistant_msg("good message 2")).unwrap();

    // Load should return exactly 2 valid messages, skipping the 2 corrupt lines
    let loaded = mgr.load(&sf.id).unwrap();
    assert_eq!(
        loaded.len(),
        2,
        "expected 2 good messages, got {}",
        loaded.len()
    );
}

#[test]
fn test_corrupt_message_field_skipped() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path().join("sessions"));

    let sf = mgr.create(dir.path()).unwrap();
    sf.append(&user_msg("before corrupt")).unwrap();

    // Inject a valid JSON line with type=message but corrupt message field
    {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&sf.path)
            .unwrap();
        // Valid JSON, valid type, but message value is unparseable as AgentMessage
        writeln!(
            f,
            r#"{{"type":"message","timestamp":"2026-01-01T00:00:00Z","message":{{"garbage":true}}}}"#
        )
        .unwrap();
    }

    sf.append(&assistant_msg("after corrupt")).unwrap();

    let loaded = mgr.load(&sf.id).unwrap();
    assert_eq!(
        loaded.len(),
        2,
        "expected 2 good messages, got {}",
        loaded.len()
    );
}

// ---------------------------------------------------------------------------
// Empty session file (header only) → loads as empty message list
// ---------------------------------------------------------------------------

#[test]
fn test_empty_session_loads_as_empty() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path().join("sessions"));

    let sf = mgr.create(dir.path()).unwrap();
    // Don't append anything

    let loaded = mgr.load(&sf.id).unwrap();
    assert!(
        loaded.is_empty(),
        "expected empty list for header-only session"
    );
}

// ---------------------------------------------------------------------------
// Missing session directory → created on first write
// ---------------------------------------------------------------------------

#[test]
fn test_creates_session_dir_if_missing() {
    let dir = tempdir().unwrap();
    let session_dir = dir.path().join("nested").join("sessions");
    assert!(!session_dir.exists(), "dir should not exist yet");

    let mgr = SessionManager::new(session_dir.clone());
    let sf = mgr.create(dir.path()).unwrap();

    assert!(session_dir.exists(), "session dir should have been created");
    assert!(sf.path.exists(), "session file should exist");
}

// ---------------------------------------------------------------------------
// --session nonexistent_id → clear error
// ---------------------------------------------------------------------------

#[test]
fn test_load_nonexistent_session_returns_error() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path().join("sessions"));
    let _ = mgr.create(dir.path()).unwrap(); // ensure dir exists

    let result = mgr.load("deadbeef");
    assert!(result.is_err(), "loading nonexistent session should fail");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("deadbeef"),
        "error should mention the session ID: {}",
        err_msg
    );
}

// ---------------------------------------------------------------------------
// latest() returns most recently modified session
// ---------------------------------------------------------------------------

#[test]
fn test_latest_returns_most_recent_session() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path().join("sessions"));

    let sf1 = mgr.create(dir.path()).unwrap();
    // Small sleep to ensure different mtime
    std::thread::sleep(std::time::Duration::from_millis(10));
    let sf2 = mgr.create(dir.path()).unwrap();

    let latest = mgr.latest().unwrap();
    assert_eq!(
        latest.as_deref(),
        Some(sf2.id.as_str()),
        "latest should be sf2, got {:?}",
        latest
    );
    let _ = sf1; // ensure sf1 is not dropped before latest() is called
}

#[test]
fn test_latest_returns_none_when_no_sessions() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path().join("sessions"));
    // Don't create any session files

    let latest = mgr.latest().unwrap();
    assert!(latest.is_none(), "expected None, got {:?}", latest);
}

// ---------------------------------------------------------------------------
// CLI flag parsing for session flags
// ---------------------------------------------------------------------------

#[test]
fn test_cli_session_flag() {
    use clap::Parser;
    use coding_agent::cli::Cli;

    let cli = Cli::try_parse_from(["coding-agent", "--session", "abc12345"]).unwrap();
    assert_eq!(cli.session.as_deref(), Some("abc12345"));
    assert!(!cli.resume);
    assert!(!cli.no_session);
}

#[test]
fn test_cli_resume_flag() {
    use clap::Parser;
    use coding_agent::cli::Cli;

    let cli = Cli::try_parse_from(["coding-agent", "--resume"]).unwrap();
    assert!(cli.resume);
    assert!(cli.session.is_none());
    assert!(!cli.no_session);
}

#[test]
fn test_cli_no_session_flag() {
    use clap::Parser;
    use coding_agent::cli::Cli;

    let cli = Cli::try_parse_from(["coding-agent", "--no-session"]).unwrap();
    assert!(cli.no_session);
    assert!(cli.session.is_none());
    assert!(!cli.resume);
}

#[test]
fn test_cli_default_is_ephemeral() {
    use clap::Parser;
    use coding_agent::cli::Cli;

    let cli = Cli::try_parse_from(["coding-agent"]).unwrap();
    assert!(cli.session.is_none());
    assert!(!cli.resume);
    assert!(!cli.no_session);
}

#[test]
fn test_cli_session_and_resume_conflict() {
    use clap::Parser;
    use coding_agent::cli::Cli;

    let result = Cli::try_parse_from(["coding-agent", "--session", "abc12345", "--resume"]);
    assert!(result.is_err(), "--session and --resume should conflict");
}

#[test]
fn test_cli_session_and_no_session_conflict() {
    use clap::Parser;
    use coding_agent::cli::Cli;

    let result = Cli::try_parse_from(["coding-agent", "--session", "abc12345", "--no-session"]);
    assert!(
        result.is_err(),
        "--session and --no-session should conflict"
    );
}
