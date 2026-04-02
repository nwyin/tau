use std::sync::Arc;

use agent::types::{AgentTool, AgentToolResult, BoxFuture};
use ai::types::UserBlock;
use anyhow::Result;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

const MAX_BODY_BYTES: usize = 5 * 1024 * 1024; // 5MB
const MAX_OUTPUT_BYTES: usize = 50 * 1024; // 50KB
const MAX_OUTPUT_LINES: usize = 2000;

pub struct WebFetchTool;

impl AgentTool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn label(&self) -> &str {
        "Web Fetch"
    }

    fn description(&self) -> &str {
        "Fetch a URL and return its content as readable text. Useful for reading documentation, API references, error explanations, and web pages."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "URL to fetch (must start with http:// or https://)"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["text", "html"],
                        "description": "Output format (default: text). 'text' extracts readable content, 'html' returns raw HTML."
                    }
                },
                "required": ["url"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        signal: Option<CancellationToken>,
    ) -> BoxFuture<Result<AgentToolResult>> {
        Box::pin(async move {
            let url = params["url"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("missing 'url' parameter"))?
                .to_string();

            // Validate URL scheme
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: format!(
                            "Invalid URL: must start with http:// or https://. Got: {}",
                            url
                        ),
                    }],
                    details: None,
                });
            }

            let want_html = params["format"].as_str() == Some("html");

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .user_agent("Mozilla/5.0 (compatible; tau/0.1; +https://github.com/anthropics/tau)")
                .default_headers({
                    let mut headers = reqwest::header::HeaderMap::new();
                    headers.insert(
                        reqwest::header::ACCEPT,
                        "text/html,text/plain,application/json,*/*"
                            .parse()
                            .expect("valid header value"),
                    );
                    headers
                })
                .build()?;

            let fetch_result = tokio::select! {
                result = client.get(&url).send() => result,
                _ = async {
                    if let Some(sig) = &signal {
                        sig.cancelled().await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {
                    return Ok(AgentToolResult {
                        content: vec![UserBlock::Text {
                            text: "Request aborted".to_string(),
                        }],
                        details: None,
                    });
                }
            };

            let response = match fetch_result {
                Ok(r) => r,
                Err(e) => {
                    let msg = if e.is_timeout() {
                        "Request timed out after 30 seconds".to_string()
                    } else if e.is_connect() {
                        format!("Connection error: {}", e)
                    } else {
                        format!("Network error: {}", e)
                    };
                    return Ok(AgentToolResult {
                        content: vec![UserBlock::Text { text: msg }],
                        details: None,
                    });
                }
            };

            let status = response.status();

            // Check Content-Length before reading
            if let Some(content_len) = response.content_length() {
                if content_len as usize > MAX_BODY_BYTES {
                    return Ok(AgentToolResult {
                        content: vec![UserBlock::Text {
                            text: "Response exceeds 5MB limit".to_string(),
                        }],
                        details: None,
                    });
                }
            }

            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_lowercase();

            if !status.is_success() {
                return Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: format!("HTTP {}: {}", status.as_u16(), url),
                    }],
                    details: None,
                });
            }

            // Read body with size limit
            let bytes = match read_body_limited(response, MAX_BODY_BYTES).await {
                Ok(b) => b,
                Err(e) => {
                    return Ok(AgentToolResult {
                        content: vec![UserBlock::Text { text: e }],
                        details: None,
                    });
                }
            };

            let body = String::from_utf8_lossy(&bytes).into_owned();

            let is_html = content_type.contains("text/html");

            // If want_html or not HTML content-type, return as-is
            let text = if want_html || !is_html {
                body
            } else {
                strip_html(&body)
            };

            let output = truncate_output(text);

            Ok(AgentToolResult {
                content: vec![UserBlock::Text { text: output }],
                details: None,
            })
        })
    }
}

async fn read_body_limited(
    response: reqwest::Response,
    limit: usize,
) -> std::result::Result<Vec<u8>, String> {
    let mut stream = response;
    let mut buf = Vec::new();

    // Use bytes() chunks approach
    while let Some(chunk) = match stream.chunk().await {
        Ok(c) => c,
        Err(e) => return Err(format!("Network error while reading response: {}", e)),
    } {
        buf.extend_from_slice(&chunk);
        if buf.len() > limit {
            return Err("Response exceeds 5MB limit".to_string());
        }
    }

    Ok(buf)
}

/// Strip HTML tags and decode common entities, removing script/style blocks entirely.
pub fn strip_html(html: &str) -> String {
    // Remove <script ...>...</script> blocks (case-insensitive)
    let without_scripts = remove_block(html, "script");
    // Remove <style ...>...</style> blocks
    let without_styles = remove_block(&without_scripts, "style");

    // Strip remaining tags
    let mut result = String::with_capacity(without_styles.len());
    let mut in_tag = false;
    for ch in without_styles.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                result.push(' ');
            }
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    // Decode common HTML entities
    let result = result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");

    // Collapse whitespace: replace runs of whitespace (including newlines) with a single space,
    // but preserve paragraph breaks (double newlines)
    collapse_whitespace(&result)
}

/// Remove all occurrences of <tag ...>...</tag> (case-insensitive).
fn remove_block(html: &str, tag: &str) -> String {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let html_lower = html.to_lowercase();
    let open_lower = open.to_lowercase();
    let close_lower = close.to_lowercase();

    let mut result = String::with_capacity(html.len());
    let mut pos = 0;

    loop {
        // Find next opening tag
        match html_lower[pos..].find(&open_lower) {
            None => {
                result.push_str(&html[pos..]);
                break;
            }
            Some(rel) => {
                let abs = pos + rel;
                result.push_str(&html[pos..abs]);
                // Find the closing tag
                match html_lower[abs..].find(&close_lower) {
                    None => break, // malformed, stop
                    Some(rel_close) => {
                        pos = abs + rel_close + close_lower.len();
                    }
                }
            }
        }
    }

    result
}

fn collapse_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut prev_whitespace = false;
    let mut line_count = 0usize;

    for ch in s.chars() {
        if ch == '\n' {
            line_count += 1;
            if line_count <= 2 {
                result.push('\n');
            }
            prev_whitespace = true;
        } else if ch.is_whitespace() {
            if !prev_whitespace {
                result.push(' ');
            }
            prev_whitespace = true;
        } else {
            line_count = 0;
            prev_whitespace = false;
            result.push(ch);
        }
    }

    result.trim().to_string()
}

/// Truncate output to MAX_OUTPUT_BYTES / MAX_OUTPUT_LINES, head+tail style.
pub fn truncate_output(text: String) -> String {
    let lines: Vec<&str> = text.lines().collect();

    if lines.len() <= MAX_OUTPUT_LINES && text.len() <= MAX_OUTPUT_BYTES {
        return text;
    }

    let total_lines = lines.len();
    let total_bytes = text.len();

    // Show first half and last half, with truncation marker
    let half = MAX_OUTPUT_LINES / 2;
    let head = &lines[..half.min(lines.len())];
    let tail_start = if lines.len() > half {
        lines.len() - half
    } else {
        0
    };
    let tail = &lines[tail_start..];

    let omitted = total_lines.saturating_sub(head.len() + tail.len());
    let head_str = head.join("\n");
    let tail_str = tail.join("\n");

    format!(
        "{}\n[truncated {} lines ({} bytes total)]\n{}",
        head_str, omitted, total_bytes, tail_str
    )
}

impl WebFetchTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(WebFetchTool)
    }
}
