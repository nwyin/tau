use agent::types::AgentTool;
use agent::types::AgentToolResult;
use ai::types::UserBlock;
use coding_agent::tools::hashline;
use coding_agent::tools::{HashFileEditTool, HashFileReadTool};
use serde_json::json;
use tempfile::TempDir;

fn text_content(result: &AgentToolResult) -> String {
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

// ── HashFileReadTool tests ──────────────────────────────────────────────

#[tokio::test]
async fn test_hash_file_read_format() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "line one\nline two\nline three\n").unwrap();

    let tool = HashFileReadTool;
    let result: AgentToolResult = tool
        .execute(
            "id1".into(),
            json!({"path": path.to_str().unwrap()}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("1#"),
        "expected '1#' hash prefix in output, got: {out}"
    );
    assert!(
        out.contains(":line one"),
        "expected ':line one' content in output, got: {out}"
    );
}

#[tokio::test]
async fn test_hash_file_read_offset_limit() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "a\nb\nc\nd\ne\n").unwrap();

    let tool = HashFileReadTool;
    let result: AgentToolResult = tool
        .execute(
            "id2".into(),
            json!({"path": path.to_str().unwrap(), "offset": 2, "limit": 2}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(out.contains("2#"), "expected '2#' prefix, got: {out}");
    assert!(out.contains(":b"), "expected ':b' content, got: {out}");
    assert!(out.contains(":c"), "expected ':c' content, got: {out}");
    assert!(!out.contains(":a"), "should not contain ':a', got: {out}");
    assert!(!out.contains(":d"), "should not contain ':d', got: {out}");
}

#[tokio::test]
async fn test_hash_file_read_empty_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("empty.txt");
    std::fs::write(&path, "").unwrap();

    let tool = HashFileReadTool;
    let result: AgentToolResult = tool
        .execute(
            "id3".into(),
            json!({"path": path.to_str().unwrap()}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("(empty file)"),
        "expected '(empty file)' for empty file, got: {out}"
    );
}

#[tokio::test]
async fn test_hash_file_read_binary() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("binary.bin");
    std::fs::write(&path, b"\xff\xfe\x00\x01\x80\x90").unwrap();

    let tool = HashFileReadTool;
    let result: AgentToolResult = tool
        .execute(
            "id4".into(),
            json!({"path": path.to_str().unwrap()}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("binary"),
        "expected 'binary' in output for binary file, got: {out}"
    );
}

#[tokio::test]
async fn test_hash_file_read_not_found() {
    let tool = HashFileReadTool;
    let result: AgentToolResult = tool
        .execute(
            "id5".into(),
            json!({"path": "/nonexistent/path/file.txt"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("File not found"),
        "expected 'File not found' error, got: {out}"
    );
}

// ── HashFileEditTool tests ──────────────────────────────────────────────

#[tokio::test]
async fn test_hash_file_edit_replace_single() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "hello\nworld\nfoo\n").unwrap();

    let hash = hashline::compute_line_hash(2, "world");
    let pos = format!("2#{hash}");

    let tool = HashFileEditTool;
    let result: AgentToolResult = tool
        .execute(
            "id6".into(),
            json!({
                "path": path.to_str().unwrap(),
                "edits": [{"op": "replace", "pos": pos, "lines": ["planet"]}]
            }),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("Applied"),
        "expected success message, got: {out}"
    );

    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, "hello\nplanet\nfoo\n");
}

#[tokio::test]
async fn test_hash_file_edit_replace_range() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "a\nb\nc\nd\ne\n").unwrap();

    let h2 = hashline::compute_line_hash(2, "b");
    let h4 = hashline::compute_line_hash(4, "d");
    let pos = format!("2#{h2}");
    let end = format!("4#{h4}");

    let tool = HashFileEditTool;
    let result: AgentToolResult = tool
        .execute(
            "id7".into(),
            json!({
                "path": path.to_str().unwrap(),
                "edits": [{"op": "replace", "pos": pos, "end": end, "lines": ["replaced"]}]
            }),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("Applied"),
        "expected success message, got: {out}"
    );

    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, "a\nreplaced\ne\n");
}

#[tokio::test]
async fn test_hash_file_edit_append() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "first\nsecond\n").unwrap();

    let hash = hashline::compute_line_hash(1, "first");
    let pos = format!("1#{hash}");

    let tool = HashFileEditTool;
    let result: AgentToolResult = tool
        .execute(
            "id8".into(),
            json!({
                "path": path.to_str().unwrap(),
                "edits": [{"op": "append", "pos": pos, "lines": ["inserted"]}]
            }),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("Applied"),
        "expected success message, got: {out}"
    );

    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, "first\ninserted\nsecond\n");
}

#[tokio::test]
async fn test_hash_file_edit_prepend() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "first\nsecond\n").unwrap();

    let hash = hashline::compute_line_hash(2, "second");
    let pos = format!("2#{hash}");

    let tool = HashFileEditTool;
    let result: AgentToolResult = tool
        .execute(
            "id9".into(),
            json!({
                "path": path.to_str().unwrap(),
                "edits": [{"op": "prepend", "pos": pos, "lines": ["inserted"]}]
            }),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("Applied"),
        "expected success message, got: {out}"
    );

    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, "first\ninserted\nsecond\n");
}

#[tokio::test]
async fn test_hash_file_edit_hash_mismatch() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "hello\nworld\n").unwrap();

    let tool = HashFileEditTool;
    let result: AgentToolResult = tool
        .execute(
            "id10".into(),
            json!({
                "path": path.to_str().unwrap(),
                "edits": [{"op": "replace", "pos": "2#ZZ", "lines": ["x"]}]
            }),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("Hash mismatch"),
        "expected 'Hash mismatch' error, got: {out}"
    );

    // File should be unchanged
    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, "hello\nworld\n");
}

#[tokio::test]
async fn test_hash_file_edit_empty_lines_deletes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "a\nb\nc\n").unwrap();

    let hash = hashline::compute_line_hash(2, "b");
    let pos = format!("2#{hash}");

    let tool = HashFileEditTool;
    let result: AgentToolResult = tool
        .execute(
            "id11".into(),
            json!({
                "path": path.to_str().unwrap(),
                "edits": [{"op": "replace", "pos": pos, "lines": []}]
            }),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("Applied"),
        "expected success message, got: {out}"
    );

    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, "a\nc\n");
}

#[tokio::test]
async fn test_hash_file_edit_strip_prefixes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "hello\nworld\n").unwrap();

    let hash = hashline::compute_line_hash(1, "hello");
    let pos = format!("1#{hash}");

    let tool = HashFileEditTool;
    let result: AgentToolResult = tool
        .execute(
            "id12".into(),
            json!({
                "path": path.to_str().unwrap(),
                "edits": [{"op": "replace", "pos": pos, "lines": ["1#ZP:replaced"]}]
            }),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("Applied"),
        "expected success message, got: {out}"
    );

    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, "replaced\nworld\n");
}

#[tokio::test]
async fn test_hash_file_edit_multiple_edits_bottom_up() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "a\nb\nc\nd\n").unwrap();

    let h1 = hashline::compute_line_hash(1, "a");
    let h3 = hashline::compute_line_hash(3, "c");
    let pos1 = format!("1#{h1}");
    let pos3 = format!("3#{h3}");

    let tool = HashFileEditTool;
    let result: AgentToolResult = tool
        .execute(
            "id13".into(),
            json!({
                "path": path.to_str().unwrap(),
                "edits": [
                    {"op": "replace", "pos": pos1, "lines": ["A"]},
                    {"op": "replace", "pos": pos3, "lines": ["C"]}
                ]
            }),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("Applied"),
        "expected success message, got: {out}"
    );

    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, "A\nb\nC\nd\n");
}

#[tokio::test]
async fn test_hash_file_edit_append_no_pos() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "a\nb\n").unwrap();

    let tool = HashFileEditTool;
    let result: AgentToolResult = tool
        .execute(
            "id14".into(),
            json!({
                "path": path.to_str().unwrap(),
                "edits": [{"op": "append", "lines": ["c"]}]
            }),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("Applied"),
        "expected success message, got: {out}"
    );

    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, "a\nb\nc\n");
}

#[tokio::test]
async fn test_hash_file_edit_prepend_no_pos() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "a\nb\n").unwrap();

    let tool = HashFileEditTool;
    let result: AgentToolResult = tool
        .execute(
            "id15".into(),
            json!({
                "path": path.to_str().unwrap(),
                "edits": [{"op": "prepend", "lines": ["z"]}]
            }),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("Applied"),
        "expected success message, got: {out}"
    );

    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, "z\na\nb\n");
}

#[tokio::test]
async fn test_hash_file_read_edit_round_trip() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "alpha\nbeta\ngamma\ndelta\n").unwrap();

    // Step 1: Read the file with HashFileReadTool
    let read_tool = HashFileReadTool;
    let read_result: AgentToolResult = read_tool
        .execute(
            "id16a".into(),
            json!({"path": path.to_str().unwrap()}),
            None,
            None,
        )
        .await
        .unwrap();
    let read_out = text_content(&read_result);

    // Step 2: Parse a tag from the output (find the line for "beta", line 2)
    let beta_line = read_out
        .lines()
        .find(|l| l.contains(":beta"))
        .expect("should find line containing ':beta'");
    // The tag is everything before the first ':'
    let tag = beta_line.split(':').next().unwrap();
    assert!(tag.contains('#'), "expected tag to contain '#', got: {tag}");

    // Step 3: Edit line 2 using the parsed tag
    let edit_tool = HashFileEditTool;
    let edit_result: AgentToolResult = edit_tool
        .execute(
            "id16b".into(),
            json!({
                "path": path.to_str().unwrap(),
                "edits": [{"op": "replace", "pos": tag, "lines": ["BETA"]}]
            }),
            None,
            None,
        )
        .await
        .unwrap();
    let edit_out = text_content(&edit_result);
    assert!(
        edit_out.contains("Applied"),
        "expected success message, got: {edit_out}"
    );

    // Step 4: Read again and verify the edit took effect
    let read_result2: AgentToolResult = read_tool
        .execute(
            "id16c".into(),
            json!({"path": path.to_str().unwrap()}),
            None,
            None,
        )
        .await
        .unwrap();
    let read_out2 = text_content(&read_result2);
    assert!(
        read_out2.contains(":BETA"),
        "expected edited content ':BETA' in output, got: {read_out2}"
    );
    assert!(
        !read_out2.contains(":beta"),
        "should not contain original ':beta' after edit, got: {read_out2}"
    );

    // Verify file content directly
    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, "alpha\nBETA\ngamma\ndelta\n");
}
