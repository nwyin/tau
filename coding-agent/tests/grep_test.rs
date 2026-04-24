//! Tests for GrepTool.

use agent::types::AgentTool;
use coding_agent::tools::GrepTool;
use serde_json::json;
use std::io::Write;
use tempfile::TempDir;

fn make_temp_file(dir: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    path
}

async fn grep(params: serde_json::Value) -> String {
    let tool = GrepTool::new();
    let result = tool.execute("id".to_string(), params, None).await.unwrap();
    match &result.content[0] {
        ai::types::UserBlock::Text { text } => text.clone(),
        _ => panic!("expected text block"),
    }
}

// INV: pattern match returns file:line:content format
#[tokio::test]
async fn grep_finds_pattern_with_line_numbers() {
    let dir = TempDir::new().unwrap();
    make_temp_file(&dir, "a.txt", "hello world\nfoo bar\nhello again\n");

    let output = grep(json!({
        "pattern": "hello",
        "path": dir.path().to_str().unwrap()
    }))
    .await;

    // rg --no-heading output: path:line:content
    assert!(output.contains(":1:"), "expected line 1 match");
    assert!(output.contains(":3:"), "expected line 3 match");
    assert!(output.contains("hello world"), "expected first match text");
    assert!(output.contains("hello again"), "expected second match text");
}

// INV: glob filter restricts search to matching files only
#[tokio::test]
async fn grep_glob_restricts_files() {
    let dir = TempDir::new().unwrap();
    make_temp_file(&dir, "code.rs", "fn search_me() {}");
    make_temp_file(&dir, "notes.txt", "search_me is a function");

    let output = grep(json!({
        "pattern": "search_me",
        "path": dir.path().to_str().unwrap(),
        "glob": "*.rs"
    }))
    .await;

    assert!(output.contains("code.rs"), "should find match in .rs file");
    assert!(
        !output.contains("notes.txt"),
        "should not search .txt file when glob is *.rs"
    );
}

// INV: ignore_case matches regardless of letter case
#[tokio::test]
async fn grep_ignore_case_matches() {
    let dir = TempDir::new().unwrap();
    make_temp_file(&dir, "data.txt", "Hello World\nHELLO WORLD\nhello world\n");

    let case_sensitive = grep(json!({
        "pattern": "hello",
        "path": dir.path().to_str().unwrap()
    }))
    .await;
    // Only the lowercase line matches
    assert!(case_sensitive.contains("hello world"));
    assert!(!case_sensitive.contains("Hello World"));

    let case_insensitive = grep(json!({
        "pattern": "hello",
        "path": dir.path().to_str().unwrap(),
        "ignore_case": true
    }))
    .await;
    assert!(
        case_insensitive.contains("Hello World"),
        "should match Hello World"
    );
    assert!(
        case_insensitive.contains("HELLO WORLD"),
        "should match HELLO WORLD"
    );
    assert!(
        case_insensitive.contains("hello world"),
        "should match hello world"
    );
}

// INV: context lines include surrounding lines in output
#[tokio::test]
async fn grep_context_shows_surrounding_lines() {
    let dir = TempDir::new().unwrap();
    make_temp_file(&dir, "ctx.txt", "line1\nline2\nMATCH\nline4\nline5\n");

    let output = grep(json!({
        "pattern": "MATCH",
        "path": dir.path().to_str().unwrap(),
        "context": 1
    }))
    .await;

    assert!(output.contains("MATCH"), "should contain the match");
    assert!(output.contains("line2"), "should contain line before match");
    assert!(output.contains("line4"), "should contain line after match");
}

// INV: limit truncates output and appends count message
#[tokio::test]
async fn grep_limit_truncates_output() {
    let dir = TempDir::new().unwrap();
    // 10 matching lines
    let content: String = (1..=10).map(|i| format!("match line {}\n", i)).collect();
    make_temp_file(&dir, "many.txt", &content);

    let output = grep(json!({
        "pattern": "match line",
        "path": dir.path().to_str().unwrap(),
        "limit": 3
    }))
    .await;

    // Should show only 3 lines and append truncation notice
    assert!(
        output.contains("showing first 3"),
        "should say showing first 3, got: {}",
        output
    );
    // The first 3 match lines should appear
    assert!(output.contains("match line 1"));
    assert!(output.contains("match line 2"));
    assert!(output.contains("match line 3"));
    // line 10 should not appear (truncated)
    assert!(
        !output.contains("match line 10"),
        "line 10 should be truncated"
    );
}

// INV: no matches returns "No matches found."
#[tokio::test]
async fn grep_no_matches_returns_message() {
    let dir = TempDir::new().unwrap();
    make_temp_file(&dir, "empty.txt", "nothing here\n");

    let output = grep(json!({
        "pattern": "XYZZY_NOT_PRESENT",
        "path": dir.path().to_str().unwrap()
    }))
    .await;

    assert_eq!(output, "No matches found.");
}

// INV: invalid regex returns error output (rg exit code 2)
#[tokio::test]
async fn grep_invalid_regex_returns_error() {
    let dir = TempDir::new().unwrap();
    make_temp_file(&dir, "file.txt", "content\n");

    // An unclosed bracket is an invalid regex in rg
    let output = grep(json!({
        "pattern": "[invalid",
        "path": dir.path().to_str().unwrap()
    }))
    .await;

    // rg exits with code 2 for bad regex, we return stderr
    // The output should not be "No matches found." — it should be an error message
    assert_ne!(
        output, "No matches found.",
        "invalid regex should return error, not 'no matches'"
    );
    assert!(!output.is_empty(), "error output should not be empty");
}

// INV: nonexistent path returns error
#[tokio::test]
async fn grep_nonexistent_path_returns_error() {
    let output = grep(json!({
        "pattern": "anything",
        "path": "/nonexistent/path/that/does/not/exist/xyz"
    }))
    .await;

    // rg returns exit code 2 for bad paths, returning stderr
    assert_ne!(output, "No matches found.");
    assert!(
        !output.is_empty(),
        "should return some error for nonexistent path"
    );
}
