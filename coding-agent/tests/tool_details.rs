//! Tests for AgentToolResult.details metadata populated by built-in tools.
//!
//! Invariants:
//! - INV-1: Every successful tool execution returns Some(details), never None
//! - INV-2: Details contain all documented fields for that tool
//! - INV-3: Numeric fields are accurate, not hardcoded

use agent::types::AgentTool;
use coding_agent::tools::{BashTool, FileEditTool, FileReadTool, FileWriteTool, GlobTool};
use serde_json::json;
use tempfile::TempDir;

fn details(result: &agent::types::AgentToolResult) -> &serde_json::Value {
    result
        .details
        .as_ref()
        .expect("expected Some(details), got None")
}

// ── file_read ──────────────────────────────────────────────────────────────

// INV-1/2/3: file_read returns correct lines_returned and total_lines
#[tokio::test]
async fn file_read_details_correct_line_counts() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("f.txt");
    std::fs::write(&path, "a\nb\nc\nd\ne\n").unwrap(); // 5 lines

    let result = FileReadTool::default()
        .execute("id".into(), json!({"path": path.to_str().unwrap()}), None)
        .await
        .unwrap();

    let d = details(&result);
    assert_eq!(d["total_lines"], 5, "total_lines should be 5");
    assert_eq!(d["lines_returned"], 5, "lines_returned should be 5");
    assert!(d["path"].is_string(), "path field should be a string");
    assert!(d["offset"].is_number(), "offset field should be a number");
    assert!(d["limit"].is_number(), "limit field should be a number");
}

// INV-3: lines_returned respects limit parameter
#[tokio::test]
async fn file_read_details_limited_lines_returned() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("f.txt");
    std::fs::write(&path, "a\nb\nc\nd\ne\n").unwrap(); // 5 lines

    let result = FileReadTool::default()
        .execute(
            "id".into(),
            json!({"path": path.to_str().unwrap(), "offset": 2, "limit": 2}),
            None,
        )
        .await
        .unwrap();

    let d = details(&result);
    assert_eq!(
        d["total_lines"], 5,
        "total_lines should be 5 regardless of limit"
    );
    assert_eq!(d["lines_returned"], 2, "lines_returned should match limit");
    assert_eq!(d["offset"], 2, "offset should be reflected in details");
    assert_eq!(d["limit"], 2, "limit should be reflected in details");
}

// error path: nonexistent file — details can be None
#[tokio::test]
async fn file_read_nonexistent_details_none() {
    let result = FileReadTool::default()
        .execute("id".into(), json!({"path": "/nonexistent/xyz.txt"}), None)
        .await
        .unwrap();
    // No assertion on details — None is acceptable for error paths
    // Just ensure the tool doesn't panic
    let _ = result.details;
}

// ── file_write ─────────────────────────────────────────────────────────────

// INV-1/2/3: file_write details on create
#[tokio::test]
async fn file_write_details_create() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("new.txt");

    let result = (FileWriteTool::new())
        .execute(
            "id".into(),
            json!({"path": path.to_str().unwrap(), "content": "hello world"}),
            None,
        )
        .await
        .unwrap();

    let d = details(&result);
    assert_eq!(
        d["bytes_written"], 11,
        "bytes_written should equal content length"
    );
    assert_eq!(d["created"], true, "created should be true for new file");
    assert!(d["path"].is_string(), "path field should be present");
}

// INV-3: created=false when file already existed
#[tokio::test]
async fn file_write_details_overwrite() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("existing.txt");
    std::fs::write(&path, "old").unwrap();

    let result = (FileWriteTool::new())
        .execute(
            "id".into(),
            json!({"path": path.to_str().unwrap(), "content": "new content"}),
            None,
        )
        .await
        .unwrap();

    let d = details(&result);
    assert_eq!(
        d["created"], false,
        "created should be false when overwriting"
    );
    assert_eq!(
        d["bytes_written"], 11,
        "bytes_written should reflect new content length"
    );
}

// ── file_edit ──────────────────────────────────────────────────────────────

// INV-1/2/3: successful replacement → success: true, replacements: 1
#[tokio::test]
async fn file_edit_details_success() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("e.txt");
    std::fs::write(&path, "foo\nbar\n").unwrap();

    let result = FileEditTool::default()
        .execute(
            "id".into(),
            json!({"path": path.to_str().unwrap(), "old_string": "bar", "new_string": "baz"}),
            None,
        )
        .await
        .unwrap();

    let d = details(&result);
    assert_eq!(d["success"], true, "success should be true on replacement");
    assert_eq!(d["replacements"], 1, "replacements should be 1");
    assert!(d["path"].is_string(), "path field should be present");
}

// INV-1/2: no match → success: false, replacements: 0
#[tokio::test]
async fn file_edit_details_no_match() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("e.txt");
    std::fs::write(&path, "hello world\n").unwrap();

    let result = FileEditTool::default()
        .execute(
            "id".into(),
            json!({"path": path.to_str().unwrap(), "old_string": "xyz", "new_string": "abc"}),
            None,
        )
        .await
        .unwrap();

    let d = details(&result);
    assert_eq!(
        d["success"], false,
        "success should be false when string not found"
    );
    assert_eq!(
        d["replacements"], 0,
        "replacements should be 0 when not found"
    );
}

// ── glob ───────────────────────────────────────────────────────────────────

// INV-1/2/3: glob result_count matches actual files found
#[tokio::test]
async fn glob_details_correct_result_count() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("a.txt"), "").unwrap();
    std::fs::write(dir.path().join("b.txt"), "").unwrap();
    std::fs::write(dir.path().join("c.rs"), "").unwrap();

    let result = (GlobTool::new())
        .execute(
            "id".into(),
            json!({"pattern": "*.txt", "path": dir.path().to_str().unwrap()}),
            None,
        )
        .await
        .unwrap();

    let d = details(&result);
    assert_eq!(
        d["result_count"], 2,
        "result_count should be 2 for 2 .txt files"
    );
    assert_eq!(
        d["truncated"], false,
        "truncated should be false for small result set"
    );
    assert!(d["pattern"].is_string(), "pattern field should be present");
    assert!(d["root"].is_string(), "root field should be present");
}

// INV-1/2: glob with zero matches still returns Some(details)
#[tokio::test]
async fn glob_details_zero_matches() {
    let dir = TempDir::new().unwrap();

    let result = (GlobTool::new())
        .execute(
            "id".into(),
            json!({"pattern": "*.nonexistent", "path": dir.path().to_str().unwrap()}),
            None,
        )
        .await
        .unwrap();

    let d = details(&result);
    assert_eq!(
        d["result_count"], 0,
        "result_count should be 0 for no matches"
    );
    assert_eq!(d["truncated"], false, "truncated should be false");
}

// ── bash ───────────────────────────────────────────────────────────────────

// INV-1/2/3: bash details on successful command
#[tokio::test]
async fn bash_details_success() {
    let result = (BashTool::new())
        .execute("id".into(), json!({"command": "echo hello"}), None)
        .await
        .unwrap();

    let d = details(&result);
    assert_eq!(d["exit_code"], 0, "exit_code should be 0 for echo");
    assert!(
        d["duration_ms"].is_number(),
        "duration_ms should be a number"
    );
    assert!(
        d["stdout_lines"].is_number(),
        "stdout_lines should be a number"
    );
    assert!(
        d["stderr_lines"].is_number(),
        "stderr_lines should be a number"
    );
    assert!(d["command"].is_string(), "command field should be present");
    assert_eq!(d["stdout_lines"], 1, "echo hello produces 1 stdout line");
}

// INV-3: exit_code reflects actual non-zero exit
#[tokio::test]
async fn bash_details_nonzero_exit() {
    let result = (BashTool::new())
        .execute("id".into(), json!({"command": "exit 42"}), None)
        .await
        .unwrap();

    let d = details(&result);
    assert_eq!(
        d["exit_code"], 42,
        "exit_code should match the shell exit code"
    );
}

// INV-3: stdout_lines and stderr_lines are accurate
#[tokio::test]
async fn bash_details_line_counts() {
    let result = (BashTool::new())
        .execute(
            "id".into(),
            json!({"command": "echo line1; echo line2; echo line3 >&2"}),
            None,
        )
        .await
        .unwrap();

    let d = details(&result);
    assert_eq!(d["stdout_lines"], 2, "stdout should have 2 lines");
    assert_eq!(d["stderr_lines"], 1, "stderr should have 1 line");
}
