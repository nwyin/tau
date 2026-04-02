use agent::types::AgentTool;
use ai::types::UserBlock;
use coding_agent::tools::GlobTool;
use serde_json::json;
use tempfile::TempDir;

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

// INV-1: Pattern **/*.rs finds Rust files in nested directories
#[tokio::test]
async fn test_glob_finds_rust_files_in_nested_dirs() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create nested structure with .rs files
    std::fs::create_dir_all(root.join("src/util")).unwrap();
    std::fs::write(root.join("main.rs"), "fn main() {}").unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub mod util;").unwrap();
    std::fs::write(root.join("src/util/helper.rs"), "pub fn help() {}").unwrap();
    // A non-.rs file that should NOT match
    std::fs::write(root.join("README.md"), "readme").unwrap();

    let tool = GlobTool;
    let result = tool
        .execute(
            "id1".into(),
            json!({"pattern": "**/*.rs", "path": root.to_str().unwrap()}),
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);

    assert!(out.contains("main.rs"), "should find main.rs, got:\n{out}");
    assert!(
        out.contains("lib.rs"),
        "should find src/lib.rs, got:\n{out}"
    );
    assert!(
        out.contains("helper.rs"),
        "should find src/util/helper.rs, got:\n{out}"
    );
    assert!(
        !out.contains("README.md"),
        "should NOT find README.md, got:\n{out}"
    );
}

// INV-2: Results are sorted by mtime descending (newest first)
#[tokio::test]
async fn test_glob_results_sorted_by_mtime_newest_first() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    std::fs::write(root.join("first.txt"), "a").unwrap();
    // Small sleep to ensure different filesystem timestamps
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(root.join("second.txt"), "b").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(root.join("third.txt"), "c").unwrap();

    let tool = GlobTool;
    let result = tool
        .execute(
            "id2".into(),
            json!({"pattern": "*.txt", "path": root.to_str().unwrap()}),
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);

    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3, "expected 3 results, got:\n{out}");

    let pos_third = lines.iter().position(|l| l.contains("third.txt")).unwrap();
    let pos_second = lines.iter().position(|l| l.contains("second.txt")).unwrap();
    let pos_first = lines.iter().position(|l| l.contains("first.txt")).unwrap();

    assert!(
        pos_third < pos_second,
        "third.txt (newest) should come before second.txt"
    );
    assert!(
        pos_second < pos_first,
        "second.txt should come before first.txt (oldest)"
    );
}

// INV-3: GlobTool respects .gitignore — ignored files are excluded
#[tokio::test]
async fn test_glob_respects_gitignore() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create .gitignore that excludes target/ and *.log
    std::fs::write(root.join(".gitignore"), "target/\n*.log\n").unwrap();
    std::fs::create_dir_all(root.join("target")).unwrap();
    std::fs::write(root.join("target/output.rs"), "ignored").unwrap();
    std::fs::write(root.join("debug.log"), "also ignored").unwrap();
    std::fs::write(root.join("main.rs"), "not ignored").unwrap();

    let tool = GlobTool;
    let result = tool
        .execute(
            "id3".into(),
            json!({"pattern": "**/*", "path": root.to_str().unwrap()}),
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);

    assert!(
        out.contains("main.rs"),
        "main.rs should be included, got:\n{out}"
    );
    assert!(
        !out.contains("output.rs"),
        "target/output.rs should be gitignored, got:\n{out}"
    );
    assert!(
        !out.contains("debug.log"),
        "debug.log should be gitignored, got:\n{out}"
    );
}

// INV-4: No matches returns a helpful message containing the pattern
#[tokio::test]
async fn test_glob_no_matches_returns_helpful_message() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    std::fs::write(root.join("readme.md"), "hello").unwrap();

    let tool = GlobTool;
    let result = tool
        .execute(
            "id4".into(),
            json!({"pattern": "**/*.go", "path": root.to_str().unwrap()}),
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);

    assert!(
        out.contains("No files matched"),
        "should report no matches, got:\n{out}"
    );
    assert!(
        out.contains("**/*.go"),
        "error message should include the pattern, got:\n{out}"
    );
}

// INV-5: Explicit path parameter scopes search to that directory only
#[tokio::test]
async fn test_glob_explicit_path_scopes_search() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    std::fs::create_dir_all(root.join("subdir")).unwrap();
    std::fs::write(root.join("root_file.txt"), "root").unwrap();
    std::fs::write(root.join("subdir/sub_file.txt"), "sub").unwrap();

    let tool = GlobTool;
    // Search only within subdir
    let result = tool
        .execute(
            "id5".into(),
            json!({"pattern": "*.txt", "path": root.join("subdir").to_str().unwrap()}),
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);

    assert!(
        out.contains("sub_file.txt"),
        "should find sub_file.txt in subdir, got:\n{out}"
    );
    assert!(
        !out.contains("root_file.txt"),
        "should NOT find root_file.txt outside search path, got:\n{out}"
    );
}

// INV-6: Single file pattern (no wildcards) finds exact match
#[tokio::test]
async fn test_glob_single_file_pattern_finds_exact_match() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    std::fs::write(root.join("Cargo.toml"), "[package]").unwrap();
    std::fs::write(root.join("Cargo.lock"), "lock contents").unwrap();

    let tool = GlobTool;
    let result = tool
        .execute(
            "id6".into(),
            json!({"pattern": "Cargo.toml", "path": root.to_str().unwrap()}),
            None,
        )
        .await
        .unwrap();
    let out = text_content(&result);

    assert!(
        out.contains("Cargo.toml"),
        "should find Cargo.toml, got:\n{out}"
    );
    assert!(
        !out.contains("Cargo.lock"),
        "should NOT find Cargo.lock, got:\n{out}"
    );
}
