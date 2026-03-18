use agent::types::AgentTool;
use agent::types::AgentToolResult;
use ai::types::UserBlock;
use coding_agent::tools::{BashTool, FileReadTool, FileWriteTool};
use serde_json::json;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

fn text_content(result: &agent::types::AgentToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|b| match b {
            UserBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

// INV-1: BashTool executes "echo hello" and returns output
#[tokio::test]
async fn test_bash_echo() {
    let tool = BashTool;
    let result: AgentToolResult = tool
        .execute(
            "id1".into(),
            json!({"command": "echo hello"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(out.contains("hello"), "expected 'hello' in output, got: {out}");
}

// INV-2: BashTool returns exit code info for failing commands
#[tokio::test]
async fn test_bash_exit_code() {
    let tool = BashTool;
    let result: AgentToolResult = tool
        .execute(
            "id2".into(),
            json!({"command": "exit 1"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("Exit code: 1"),
        "expected 'Exit code: 1' in output, got: {out}"
    );
}

// INV-3: BashTool respects timeout
#[tokio::test]
async fn test_bash_timeout() {
    let tool = BashTool;
    let result: AgentToolResult = tool
        .execute(
            "id3".into(),
            json!({"command": "sleep 10", "timeout": 1}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("timed out"),
        "expected timeout message, got: {out}"
    );
}

// INV-4: BashTool respects CancellationToken
#[tokio::test]
async fn test_bash_cancellation() {
    let tool = BashTool;
    let ct = CancellationToken::new();
    let ct_clone = ct.clone();

    // Cancel after a short delay
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        ct_clone.cancel();
    });

    let result: AgentToolResult = tool
        .execute(
            "id4".into(),
            json!({"command": "sleep 10"}),
            Some(ct),
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("aborted"),
        "expected 'aborted' in output, got: {out}"
    );
}

// INV-5: FileReadTool reads a file and returns numbered lines
#[tokio::test]
async fn test_file_read_numbered_lines() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "line one\nline two\nline three\n").unwrap();

    let tool = FileReadTool;
    let result: AgentToolResult = tool
        .execute(
            "id5".into(),
            json!({"path": path.to_str().unwrap()}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(out.contains("1\tline one"), "expected numbered lines, got: {out}");
    assert!(out.contains("2\tline two"), "expected numbered lines, got: {out}");
    assert!(out.contains("3\tline three"), "expected numbered lines, got: {out}");
}

// INV-6: FileReadTool respects offset and limit
#[tokio::test]
async fn test_file_read_offset_limit() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "a\nb\nc\nd\ne\n").unwrap();

    let tool = FileReadTool;
    let result: AgentToolResult = tool
        .execute(
            "id6".into(),
            json!({"path": path.to_str().unwrap(), "offset": 2, "limit": 2}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    // Should show lines 2 and 3 (b, c)
    assert!(out.contains("2\tb"), "expected line 2, got: {out}");
    assert!(out.contains("3\tc"), "expected line 3, got: {out}");
    assert!(!out.contains("1\ta"), "should not contain line 1, got: {out}");
    assert!(!out.contains("4\td"), "should not contain line 4, got: {out}");
}

// INV-7: FileReadTool returns error for nonexistent files
#[tokio::test]
async fn test_file_read_not_found() {
    let tool = FileReadTool;
    let result: AgentToolResult = tool
        .execute(
            "id7".into(),
            json!({"path": "/nonexistent/path/that/does/not/exist.txt"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("File not found"),
        "expected not found error, got: {out}"
    );
}

// INV-8: FileReadTool returns error for binary files
#[tokio::test]
async fn test_file_read_binary() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("binary.bin");
    // Write invalid UTF-8 bytes
    std::fs::write(&path, b"\xff\xfe\x00\x01\x80\x90").unwrap();

    let tool = FileReadTool;
    let result: AgentToolResult = tool
        .execute(
            "id8".into(),
            json!({"path": path.to_str().unwrap()}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("binary"),
        "expected binary file error, got: {out}"
    );
}

// INV-9: FileWriteTool creates file with correct content
#[tokio::test]
async fn test_file_write_creates_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("output.txt");

    let tool = FileWriteTool;
    let result: AgentToolResult = tool
        .execute(
            "id9".into(),
            json!({"path": path.to_str().unwrap(), "content": "hello world"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(out.contains("Wrote"), "expected success message, got: {out}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello world");
}

// INV-10: FileWriteTool creates parent directories
#[tokio::test]
async fn test_file_write_creates_parents() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("a").join("b").join("c").join("file.txt");

    let tool = FileWriteTool;
    let result: AgentToolResult = tool
        .execute(
            "id10".into(),
            json!({"path": path.to_str().unwrap(), "content": "nested"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(out.contains("Wrote"), "expected success message, got: {out}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "nested");
}

// INV-11: FileWriteTool overwrites existing files
#[tokio::test]
async fn test_file_write_overwrites() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("existing.txt");
    std::fs::write(&path, "original content").unwrap();

    let tool = FileWriteTool;
    let result: AgentToolResult = tool
        .execute(
            "id11".into(),
            json!({"path": path.to_str().unwrap(), "content": "new content"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(out.contains("Wrote"), "expected success message, got: {out}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new content");
}
