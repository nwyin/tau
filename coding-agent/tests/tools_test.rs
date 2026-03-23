use agent::types::AgentTool;
use agent::types::AgentToolResult;
use ai::types::UserBlock;
use coding_agent::tools::{BashTool, FileEditTool, FileReadTool, FileWriteTool};
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
        .execute("id1".into(), json!({"command": "echo hello"}), None, None)
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("hello"),
        "expected 'hello' in output, got: {out}"
    );
}

// INV-2: BashTool returns exit code info for failing commands
#[tokio::test]
async fn test_bash_exit_code() {
    let tool = BashTool;
    let result: AgentToolResult = tool
        .execute("id2".into(), json!({"command": "exit 1"}), None, None)
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
        .execute("id4".into(), json!({"command": "sleep 10"}), Some(ct), None)
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

    let tool = FileReadTool::default();
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
    assert!(
        out.contains("1\tline one"),
        "expected numbered lines, got: {out}"
    );
    assert!(
        out.contains("2\tline two"),
        "expected numbered lines, got: {out}"
    );
    assert!(
        out.contains("3\tline three"),
        "expected numbered lines, got: {out}"
    );
}

// INV-6: FileReadTool respects offset and limit
#[tokio::test]
async fn test_file_read_offset_limit() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "a\nb\nc\nd\ne\n").unwrap();

    let tool = FileReadTool::default();
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
    assert!(
        !out.contains("1\ta"),
        "should not contain line 1, got: {out}"
    );
    assert!(
        !out.contains("4\td"),
        "should not contain line 4, got: {out}"
    );
}

// INV-7: FileReadTool returns error for nonexistent files
#[tokio::test]
async fn test_file_read_not_found() {
    let tool = FileReadTool::default();
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

    let tool = FileReadTool::default();
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
    assert!(
        out.contains("Wrote"),
        "expected success message, got: {out}"
    );
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
    assert!(
        out.contains("Wrote"),
        "expected success message, got: {out}"
    );
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
    assert!(
        out.contains("Wrote"),
        "expected success message, got: {out}"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new content");
}

// INV-12: Exact string replacement produces correct file content
#[tokio::test]
async fn test_file_edit_basic_replacement() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("edit.txt");
    std::fs::write(&path, "x=1\ny=2\n").unwrap();

    let tool = FileEditTool::default();
    let result: AgentToolResult = tool
        .execute(
            "id12".into(),
            json!({"path": path.to_str().unwrap(), "old_string": "y=2", "new_string": "y=999"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("Replaced 1 occurrence"),
        "expected success message, got: {out}"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "x=1\ny=999\n");
}

// INV-13: Multiple matches of old_string rejected with count in error message
#[tokio::test]
async fn test_file_edit_multiple_matches() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("dup.txt");
    std::fs::write(&path, "foo\nfoo\nbar\n").unwrap();

    let tool = FileEditTool::default();
    let result: AgentToolResult = tool
        .execute(
            "id13".into(),
            json!({"path": path.to_str().unwrap(), "old_string": "foo", "new_string": "baz"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("2"),
        "expected count '2' in error message, got: {out}"
    );
    assert!(
        out.contains("occurrences"),
        "expected 'occurrences' in error message, got: {out}"
    );
    // File must be unchanged
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "foo\nfoo\nbar\n");
}

// INV-14: old_string not found returns error with helpful file context
#[tokio::test]
async fn test_file_edit_not_found() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nf.txt");
    std::fs::write(&path, "hello world\nsecond line\n").unwrap();

    let tool = FileEditTool::default();
    let result: AgentToolResult = tool
        .execute(
            "id14".into(),
            json!({"path": path.to_str().unwrap(), "old_string": "does not exist", "new_string": "x"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("not found"),
        "expected 'not found' in error, got: {out}"
    );
    // Should include some file context to help diagnose stale edits
    assert!(
        out.contains("hello world") || out.contains("second line"),
        "expected file context lines in error, got: {out}"
    );
}

// INV-15: Non-existent file returns "not found" error
#[tokio::test]
async fn test_file_edit_nonexistent_file() {
    let tool = FileEditTool::default();
    let result: AgentToolResult = tool
        .execute(
            "id15".into(),
            json!({"path": "/nonexistent/path/file.txt", "old_string": "foo", "new_string": "bar"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("not found") || out.contains("not found"),
        "expected not-found error, got: {out}"
    );
}

// INV-16: Binary file returns error containing "binary"
#[tokio::test]
async fn test_file_edit_binary_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("binary.bin");
    std::fs::write(&path, b"\xff\xfe\x00\x01\x80\x90").unwrap();

    let tool = FileEditTool::default();
    let result: AgentToolResult = tool
        .execute(
            "id16".into(),
            json!({"path": path.to_str().unwrap(), "old_string": "foo", "new_string": "bar"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("binary"),
        "expected 'binary' in error message, got: {out}"
    );
}

// INV-17: Empty new_string deletes the matched text successfully
#[tokio::test]
async fn test_file_edit_empty_new_string_deletes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("del.txt");
    std::fs::write(&path, "keep this\ndelete me\nkeep that\n").unwrap();

    let tool = FileEditTool::default();
    let result: AgentToolResult = tool
        .execute(
            "id17".into(),
            json!({"path": path.to_str().unwrap(), "old_string": "delete me\n", "new_string": ""}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("Replaced 1 occurrence"),
        "expected success message, got: {out}"
    );
    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(after, "keep this\nkeep that\n");
}

// INV-18: Whitespace in old_string must match exactly (tabs, spaces, newlines preserved)
#[tokio::test]
async fn test_file_edit_whitespace_exact_match() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ws.txt");
    // File has a tab-indented line
    std::fs::write(&path, "fn foo() {\n\treturn 1;\n}\n").unwrap();

    let tool = FileEditTool::default();

    // Spaces instead of tab: fuzzy cascade (trim_both) recovers the match
    let result_spaces: AgentToolResult = tool
        .execute(
            "id18a".into(),
            json!({"path": path.to_str().unwrap(), "old_string": "    return 1;", "new_string": "    return 2;"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out_spaces = text_content(&result_spaces);
    assert!(
        out_spaces.contains("matched via trim_both"),
        "spaces-vs-tab should fuzzy match via trim_both, got: {out_spaces}"
    );
    // Reset file for the next sub-test
    std::fs::write(&path, "fn foo() {\n\treturn 1;\n}\n").unwrap();

    // Using the exact tab should match
    let result_tab: AgentToolResult = tool
        .execute(
            "id18b".into(),
            json!({"path": path.to_str().unwrap(), "old_string": "\treturn 1;", "new_string": "\treturn 42;"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out_tab = text_content(&result_tab);
    assert!(
        out_tab.contains("Replaced 1 occurrence"),
        "tab should match exactly, got: {out_tab}"
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "fn foo() {\n\treturn 42;\n}\n"
    );
}

// ---------------------------------------------------------------------------
// Fuzzy matching cascade tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fuzzy_trailing_whitespace_recovery() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("trail.txt");
    // File has no trailing spaces
    std::fs::write(&path, "hello world\ngoodbye\n").unwrap();

    let tool = FileEditTool::default();
    // old_string has trailing spaces on the first line
    let result: AgentToolResult = tool
        .execute(
            "fz1".into(),
            json!({"path": path.to_str().unwrap(), "old_string": "hello world   \ngoodbye", "new_string": "hi\nbye"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("matched via trim_end"),
        "trailing ws should match via trim_end, got: {out}"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hi\nbye\n");
}

#[tokio::test]
async fn test_fuzzy_unicode_recovery() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("unicode.txt");
    // File has ASCII quotes
    std::fs::write(&path, "let msg = \"hello\";\n").unwrap();

    let tool = FileEditTool::default();
    // old_string has smart quotes (common when model copies from training data)
    let result: AgentToolResult = tool
        .execute(
            "fz2".into(),
            json!({"path": path.to_str().unwrap(), "old_string": "let msg = \u{201c}hello\u{201d};", "new_string": "let msg = \"world\";"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("matched via unicode"),
        "smart quotes should match via unicode, got: {out}"
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "let msg = \"world\";\n"
    );
}

#[tokio::test]
async fn test_fuzzy_ambiguous_rejected() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ambig.txt");
    // Two identical blocks differing only by indentation
    std::fs::write(&path, "  foo()\n    foo()\n").unwrap();

    let tool = FileEditTool::default();
    // After trim_both, "foo()" appears twice — should be rejected
    let result: AgentToolResult = tool
        .execute(
            "fz3".into(),
            json!({"path": path.to_str().unwrap(), "old_string": "      foo()", "new_string": "bar()"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("not found"),
        "ambiguous fuzzy match should be rejected, got: {out}"
    );
}

#[tokio::test]
async fn test_exact_match_preferred_over_fuzzy() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("exact.txt");
    std::fs::write(&path, "  hello world\n").unwrap();

    let tool = FileEditTool::default();
    // Exact match exists — should use exact, not fuzzy
    let result: AgentToolResult = tool
        .execute(
            "fz4".into(),
            json!({"path": path.to_str().unwrap(), "old_string": "  hello world", "new_string": "  goodbye world"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("Replaced 1 occurrence") && !out.contains("matched via"),
        "exact match should not mention fuzzy strategy, got: {out}"
    );
}

#[tokio::test]
async fn test_fuzzy_indent_shift_recovery() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("indent.txt");
    // File has 2-space indent
    std::fs::write(&path, "fn main() {\n  let x = 1;\n  let y = 2;\n}\n").unwrap();

    let tool = FileEditTool::default();
    // old_string has 4-space indent
    let result: AgentToolResult = tool
        .execute(
            "fz5".into(),
            json!({"path": path.to_str().unwrap(), "old_string": "    let x = 1;\n    let y = 2;", "new_string": "  let x = 10;\n  let y = 20;"}),
            None,
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);
    assert!(
        out.contains("matched via trim_both"),
        "indent shift should match via trim_both, got: {out}"
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "fn main() {\n  let x = 10;\n  let y = 20;\n}\n"
    );
}
