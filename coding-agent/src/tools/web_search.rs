use std::sync::Arc;

use agent::types::{AgentTool, AgentToolResult, BoxFuture, ToolUpdateFn};
use ai::types::UserBlock;
use anyhow::Result;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

pub struct WebSearchTool;

impl AgentTool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn label(&self) -> &str {
        "Web Search"
    }

    fn description(&self) -> &str {
        "Search the web using the Exa API. Returns search results with titles, URLs, and text content. Requires EXA_API_KEY environment variable."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "num_results": { "type": "number", "description": "Number of results to return (default: 5, max: 10)" }
                },
                "required": ["query"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        signal: Option<CancellationToken>,
        _on_update: Option<ToolUpdateFn>,
    ) -> BoxFuture<Result<AgentToolResult>> {
        Box::pin(async move {
            let api_key = match std::env::var("EXA_API_KEY") {
                Ok(key) if !key.is_empty() => key,
                _ => {
                    return Ok(AgentToolResult {
                        content: vec![UserBlock::Text {
                            text: "EXA_API_KEY not set. Get an API key at https://exa.ai".to_string(),
                        }],
                        details: None,
                    });
                }
            };

            let query = params["query"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("missing 'query' parameter"))?
                .to_string();

            let num_results = params["num_results"]
                .as_u64()
                .unwrap_or(5)
                .min(10) as usize;

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(25))
                .build()?;

            let body = json!({
                "query": query,
                "type": "auto",
                "numResults": num_results,
                "contents": {
                    "text": { "maxCharacters": 10000 }
                }
            });

            let request = client
                .post("https://api.exa.ai/search")
                .header("x-api-key", &api_key)
                .header("Content-Type", "application/json")
                .json(&body);

            let response = tokio::select! {
                res = request.send() => {
                    match res {
                        Ok(r) => r,
                        Err(e) if e.is_timeout() => {
                            return Ok(AgentToolResult {
                                content: vec![UserBlock::Text {
                                    text: "Search timed out after 25 seconds".to_string(),
                                }],
                                details: None,
                            });
                        }
                        Err(e) => return Err(e.into()),
                    }
                },
                _ = async {
                    if let Some(sig) = &signal {
                        sig.cancelled().await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {
                    return Ok(AgentToolResult {
                        content: vec![UserBlock::Text {
                            text: "Search cancelled".to_string(),
                        }],
                        details: None,
                    });
                }
            };

            let status = response.status();
            if !status.is_success() {
                let body_text = response.text().await.unwrap_or_default();
                return Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: format!("Exa API error (HTTP {}): {}", status.as_u16(), body_text),
                    }],
                    details: None,
                });
            }

            let json_response: Value = response.json().await?;

            let results = match json_response["results"].as_array() {
                Some(r) => r,
                None => {
                    return Ok(AgentToolResult {
                        content: vec![UserBlock::Text {
                            text: format!("No results found for query: {}", query),
                        }],
                        details: None,
                    });
                }
            };

            if results.is_empty() {
                return Ok(AgentToolResult {
                    content: vec![UserBlock::Text {
                        text: format!("No results found for query: {}", query),
                    }],
                    details: None,
                });
            }

            let mut output = String::new();
            for (i, result) in results.iter().enumerate() {
                let title = result["title"].as_str().unwrap_or("(no title)");
                let url = result["url"].as_str().unwrap_or("(no url)");
                let text = result["text"].as_str().unwrap_or("");
                let text_snippet = if text.len() > 2000 { &text[..2000] } else { text };

                if i > 0 {
                    output.push_str("\n---\n\n");
                }
                output.push_str(&format!("## Result {}: {}\n", i + 1, title));
                output.push_str(&format!("URL: {}\n\n", url));
                if !text_snippet.is_empty() {
                    output.push_str(text_snippet);
                    output.push('\n');
                }
            }

            // Truncate if output is too large (50KB or 2000 lines)
            let output = truncate_output(&output, 50 * 1024, 2000);

            Ok(AgentToolResult {
                content: vec![UserBlock::Text { text: output }],
                details: Some(json!({
                    "query": query,
                    "num_results": results.len(),
                })),
            })
        })
    }
}

/// Truncate output to max_bytes or max_lines, whichever comes first.
pub fn truncate_output(text: &str, max_bytes: usize, max_lines: usize) -> String {
    if text.len() <= max_bytes {
        let line_count = text.lines().count();
        if line_count <= max_lines {
            return text.to_string();
        }
        // Truncate by lines
        let truncated: String = text.lines().take(max_lines).collect::<Vec<_>>().join("\n");
        return format!("{}\n[output truncated at {} lines]", truncated, max_lines);
    }

    // Truncate by bytes (find a valid UTF-8 boundary)
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let truncated = &text[..end];

    // Also check line count in the byte-truncated slice
    let line_count = truncated.lines().count();
    if line_count > max_lines {
        let line_truncated: String = truncated.lines().take(max_lines).collect::<Vec<_>>().join("\n");
        return format!("{}\n[output truncated at {} lines]", line_truncated, max_lines);
    }

    format!("{}\n[output truncated at {} bytes]", truncated, max_bytes)
}

impl WebSearchTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(WebSearchTool)
    }
}
