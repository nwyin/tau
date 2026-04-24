use agent::types::AgentTool;
use ai::types::UserBlock;
use coding_agent::tools::WebSearchTool;
use serde_json::json;

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

// INV-1: Missing EXA_API_KEY returns a helpful error, not a panic
#[tokio::test]
async fn test_missing_api_key_returns_helpful_error() {
    // Ensure the env var is not set for this test
    std::env::remove_var("EXA_API_KEY");

    let tool = WebSearchTool;
    let result = tool
        .execute("id1".into(), json!({"query": "rust programming"}), None)
        .await
        .unwrap();

    let out = text_content(&result);
    assert!(
        out.contains("EXA_API_KEY"),
        "error should mention EXA_API_KEY, got: {out}"
    );
    assert!(
        out.contains("https://exa.ai"),
        "error should include setup URL, got: {out}"
    );
}

// INV-3: num_results is clamped to max 10
//
// We test this by verifying the tool description and by checking that the
// tool itself doesn't panic when given a very large num_results. Since we
// can't make a live API call, we only test behavior when the API key is absent.
#[tokio::test]
async fn test_num_results_over_limit_does_not_panic() {
    std::env::remove_var("EXA_API_KEY");

    let tool = WebSearchTool;
    // num_results = 999 — should not panic; returns API key error before clamping matters
    let result = tool
        .execute(
            "id3".into(),
            json!({"query": "test query", "num_results": 999}),
            None,
        )
        .await
        .unwrap();

    let out = text_content(&result);
    // Without API key we get a graceful error, not a panic
    assert!(!out.is_empty(), "should return non-empty error message");
}

// Verify truncate_output: bytes limit
#[test]
fn test_truncate_output_by_bytes() {
    use coding_agent::tools::web_search::truncate_output;

    let text = "a".repeat(100);
    let result = truncate_output(&text, 50, 10000);
    assert!(
        result.len() < text.len(),
        "should truncate when text exceeds max_bytes"
    );
    assert!(
        result.contains("truncated"),
        "truncated result should mention truncation"
    );
}

// Verify truncate_output: lines limit
#[test]
fn test_truncate_output_by_lines() {
    use coding_agent::tools::web_search::truncate_output;

    let text = (0..100)
        .map(|i| format!("line {}\n", i))
        .collect::<String>();
    let result = truncate_output(&text, 1_000_000, 10);
    let line_count = result.lines().count();
    // Should be 10 content lines + 1 truncation notice line
    assert!(
        line_count <= 11,
        "should truncate to ~10 lines, got {line_count}"
    );
    assert!(
        result.contains("truncated"),
        "should mention truncation, got: {result}"
    );
}

// Verify truncate_output: no truncation when under limits
#[test]
fn test_truncate_output_no_truncation_when_within_limits() {
    use coding_agent::tools::web_search::truncate_output;

    let text = "hello\nworld\n";
    let result = truncate_output(text, 1_000_000, 10000);
    assert_eq!(result, text, "should not truncate when within limits");
}

// Failure mode: empty API key string is treated as missing
#[tokio::test]
async fn test_empty_api_key_returns_error() {
    std::env::set_var("EXA_API_KEY", "");

    let tool = WebSearchTool;
    let result = tool
        .execute("id4".into(), json!({"query": "test"}), None)
        .await
        .unwrap();

    let out = text_content(&result);
    assert!(
        out.contains("EXA_API_KEY"),
        "empty key should return setup instructions, got: {out}"
    );

    // Clean up
    std::env::remove_var("EXA_API_KEY");
}

// web_search is in both default tools and all_known_tools
#[test]
fn test_web_search_in_default_tools() {
    use coding_agent::tools::{all_known_tools, default_tools};

    let tools = default_tools();
    let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    assert!(
        names.contains(&"web_search"),
        "web_search should be in default tools, got: {:?}",
        names
    );
    assert!(
        names.contains(&"web_fetch"),
        "web_fetch should be in default tools, got: {:?}",
        names
    );

    let all = all_known_tools();
    assert!(
        all.contains_key("web_search"),
        "web_search must be in all_known_tools"
    );
}
