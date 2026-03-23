use agent::types::AgentTool;
use ai::types::UserBlock;
/// Tests for the web_fetch tool (all offline — no live HTTP calls).
use coding_agent::tools::web_fetch::{strip_html, truncate_output, WebFetchTool};
use serde_json::json;

// INV-3: Tool metadata is correct.
#[test]
fn tool_name_and_required_params() {
    let tool = WebFetchTool;
    assert_eq!(tool.name(), "web_fetch");
    assert_eq!(tool.label(), "Web Fetch");

    let schema = tool.parameters();
    let required = schema["required"].as_array().expect("required array");
    let req_names: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(req_names.contains(&"url"), "url must be in required");

    // format is optional
    let props = &schema["properties"];
    assert!(props["url"].is_object(), "url property must exist");
    assert!(props["format"].is_object(), "format property must exist");
    let format_enum = props["format"]["enum"].as_array().expect("enum array");
    let variants: Vec<&str> = format_enum.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(variants.contains(&"text"));
    assert!(variants.contains(&"html"));
}

// INV-1: Invalid URLs never attempt a fetch — validated synchronously via execute().
#[tokio::test]
async fn invalid_url_no_scheme_returns_error() {
    let tool = WebFetchTool;
    let result = tool
        .execute(
            "id1".to_string(),
            json!({"url": "example.com/page"}),
            None,
            None,
        )
        .await
        .expect("execute should not panic");

    let text = extract_text(&result.content);
    assert!(
        text.contains("Invalid URL") || text.contains("http"),
        "should mention invalid URL, got: {}",
        text
    );
}

#[tokio::test]
async fn ftp_url_returns_error() {
    let tool = WebFetchTool;
    let result = tool
        .execute(
            "id2".to_string(),
            json!({"url": "ftp://example.com/file.txt"}),
            None,
            None,
        )
        .await
        .expect("execute should not panic");

    let text = extract_text(&result.content);
    assert!(
        text.contains("Invalid URL") || text.contains("http"),
        "ftp scheme should be rejected, got: {}",
        text
    );
}

#[tokio::test]
async fn file_url_returns_error() {
    let tool = WebFetchTool;
    let result = tool
        .execute(
            "id3".to_string(),
            json!({"url": "file:///etc/passwd"}),
            None,
            None,
        )
        .await
        .expect("execute should not panic");

    let text = extract_text(&result.content);
    assert!(
        text.contains("Invalid URL") || text.contains("http"),
        "file scheme should be rejected, got: {}",
        text
    );
}

// HTML stripping — critical path: tags removed, entities decoded.
#[test]
fn strip_html_removes_tags() {
    let html = "<html><body><h1>Hello</h1><p>World &amp; stuff</p></body></html>";
    let text = strip_html(html);
    assert!(text.contains("Hello"), "heading text retained");
    assert!(text.contains("World & stuff"), "entity decoded");
    assert!(!text.contains('<'), "no tags remaining");
    assert!(!text.contains('>'), "no tags remaining");
}

#[test]
fn strip_html_removes_script_and_style_blocks() {
    let html = r#"<html><head>
        <style>body { color: red; }</style>
        <script>alert('xss');</script>
    </head><body><p>Content here</p></body></html>"#;
    let text = strip_html(html);
    assert!(text.contains("Content here"), "body text retained");
    assert!(!text.contains("color: red"), "style content stripped");
    assert!(!text.contains("alert"), "script content stripped");
}

#[test]
fn strip_html_decodes_entities() {
    let html = "<p>&lt;tag&gt; &quot;quoted&quot; &#39;apostrophe&#39; &nbsp;space&amp;amp</p>";
    let text = strip_html(html);
    assert!(text.contains("<tag>"), "lt/gt decoded");
    assert!(text.contains("\"quoted\""), "quot decoded");
    assert!(text.contains("'apostrophe'"), "&#39; decoded");
}

#[test]
fn strip_html_handles_plain_text() {
    let plain = "Just plain text, no tags.";
    let text = strip_html(plain);
    assert_eq!(text.trim(), "Just plain text, no tags.");
}

// INV-2: Truncation enforced.
#[test]
fn truncate_output_respects_line_limit() {
    let many_lines: String = (0..3000).map(|i| format!("line {}\n", i)).collect();
    let result = truncate_output(many_lines);
    let line_count = result.lines().count();
    // 1000 head + 1 marker + 1000 tail = 2001, but allow some slack
    assert!(
        line_count <= 2100,
        "truncated output must be near 2000 lines, got {}",
        line_count
    );
    assert!(
        result.contains("truncated"),
        "must contain truncation marker"
    );
}

#[test]
fn truncate_output_respects_byte_limit() {
    // 3000 lines * ~20 bytes = ~60KB > 50KB
    let big: String = (0..3000)
        .map(|i| format!("abcdefghijklmnop {}\n", i))
        .collect();
    let result = truncate_output(big);
    assert!(
        result.contains("truncated"),
        "must contain truncation marker"
    );
}

#[test]
fn truncate_output_short_text_unchanged() {
    let short = "Hello\nWorld\n";
    let result = truncate_output(short.to_string());
    assert_eq!(result, short);
}

#[test]
fn truncate_output_includes_head_and_tail() {
    let lines: String = (0..3000).map(|i| format!("line {}\n", i)).collect();
    let result = truncate_output(lines);
    assert!(result.contains("line 0"), "head lines present");
    assert!(result.contains("line 2999"), "tail lines present");
}

// JSON API response path: strip_html should not mangle non-HTML
#[test]
fn json_content_strip_is_identity_for_no_tags() {
    let json_body = r#"{"key": "value", "num": 42}"#;
    // strip_html should not destroy JSON content
    let result = strip_html(json_body);
    // curly braces don't get stripped (they're not HTML tags)
    assert!(result.contains("value"), "json value preserved");
    assert!(result.contains("42"), "json number preserved");
}

fn extract_text(blocks: &[UserBlock]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            UserBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}
