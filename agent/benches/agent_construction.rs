use std::sync::Arc;

use agent::agent::{Agent, AgentOptions, AgentStateInit};
use agent::types::{AgentMessage, AgentTool, AgentToolResult, BoxFuture, ToolUpdateFn};
use ai::types::{
    AssistantMessage, ContentBlock, Cost, Message, ModelCost, StopReason, ToolResultMessage, Usage,
    UserBlock, UserContent, UserMessage,
};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Mock model and tools
// ---------------------------------------------------------------------------

fn mock_model() -> ai::types::Model {
    ai::types::Model {
        id: "mock".into(),
        name: "Mock Model".into(),
        api: "openai-responses".into(),
        provider: "openai".into(),
        base_url: "https://example.invalid".into(),
        reasoning: false,
        input: vec!["text".into()],
        cost: ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 8192,
        max_tokens: 2048,
        headers: None,
        compat: None,
    }
}

struct MockTool {
    name: &'static str,
    description: &'static str,
    params: Value,
}

impl AgentTool for MockTool {
    fn name(&self) -> &str {
        self.name
    }
    fn label(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.description
    }
    fn parameters(&self) -> &Value {
        &self.params
    }
    fn execute(
        &self,
        _tool_call_id: String,
        _params: Value,
        _signal: Option<tokio_util::sync::CancellationToken>,
        _on_update: Option<ToolUpdateFn>,
    ) -> BoxFuture<anyhow::Result<AgentToolResult>> {
        Box::pin(async {
            Ok(AgentToolResult {
                content: vec![UserBlock::Text { text: "ok".into() }],
                details: None,
            })
        })
    }
}

fn mock_tools() -> Vec<Arc<dyn AgentTool>> {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "File path" }
        },
        "required": ["path"]
    });
    vec![
        Arc::new(MockTool {
            name: "read_file",
            description: "Read a file from the filesystem.",
            params: schema.clone(),
        }),
        Arc::new(MockTool {
            name: "write_file",
            description: "Write content to a file.",
            params: schema.clone(),
        }),
        Arc::new(MockTool {
            name: "bash",
            description: "Execute a bash command.",
            params: schema.clone(),
        }),
        Arc::new(MockTool {
            name: "search",
            description: "Search code using ripgrep.",
            params: schema,
        }),
    ]
}

// ---------------------------------------------------------------------------
// Message construction helper (same pattern as ai message_serde bench)
// ---------------------------------------------------------------------------

fn build_agent_messages(n: usize) -> Vec<AgentMessage> {
    let texts = [
        "Hello, how are you doing today? I have a question about Rust async patterns.",
        "Sure, I can help you with that. Rust's async system is built on futures and executors.",
        "The tokio runtime provides async I/O, timers, and task scheduling for concurrent work.",
        "Could you explain the difference between async fn and returning impl Future?",
        "An async fn desugars into a state machine implementing Future automatically by the compiler.",
    ];

    let mut msgs = Vec::with_capacity(n);
    for i in 0..n {
        let msg = match i % 3 {
            0 => {
                let text = texts[i % texts.len()];
                Message::User(UserMessage {
                    role: "user".into(),
                    content: UserContent::Text(format!("{} (turn {})", text, i)),
                    timestamp: 1_700_000_000_000 + i as i64 * 1000,
                })
            }
            1 => Message::Assistant(AssistantMessage {
                role: "assistant".into(),
                content: vec![ContentBlock::Text {
                    text: format!("Assistant response for turn {}.", i),
                    text_signature: None,
                }],
                api: "anthropic-messages".into(),
                provider: "anthropic".into(),
                model: "claude-sonnet-4-5".into(),
                usage: Usage {
                    input: 1200 + i as u64,
                    output: 300 + i as u64,
                    cache_read: 0,
                    cache_write: 0,
                    total_tokens: 1500 + i as u64,
                    cost: Cost {
                        input: 0.003,
                        output: 0.015,
                        cache_read: 0.0,
                        cache_write: 0.0,
                        total: 0.018,
                    },
                },
                stop_reason: StopReason::Stop,
                error_message: None,
                timestamp: 1_700_000_000_500 + i as i64 * 1000,
            }),
            _ => Message::ToolResult(ToolResultMessage {
                role: "toolResult".into(),
                tool_call_id: format!("call_{:04x}", i),
                tool_name: "read_file".into(),
                content: vec![UserBlock::Text {
                    text: format!("result for call {} ", i),
                }],
                details: None,
                is_error: false,
                timestamp: 1_700_000_001_000 + i as i64 * 1000,
            }),
        };
        msgs.push(AgentMessage::Llm(msg));
    }
    msgs
}

fn new_agent() -> Agent {
    Agent::new(AgentOptions {
        initial_state: Some(AgentStateInit {
            system_prompt: Some("You are a helpful coding assistant.".into()),
            model: Some(mock_model()),
            thinking_level: None,
            tools: Some(mock_tools()),
        }),
        convert_to_llm: None,
        transform_context: None,
        stream_fn: None,
        steering_mode: None,
        follow_up_mode: None,
        session_id: None,
        get_api_key: None,
        thinking_budgets: None,
        transport: None,
        max_retry_delay_ms: None,
    })
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_new_agent(c: &mut Criterion) {
    let mut group = c.benchmark_group("agent_construction");
    group.bench_function("new_agent", |b| {
        b.iter(new_agent);
    });
    group.finish();
}

fn bench_replace_messages(c: &mut Criterion) {
    let mut group = c.benchmark_group("agent_construction");
    for n in [10usize, 50, 100, 500] {
        let messages = build_agent_messages(n);
        group.bench_with_input(
            BenchmarkId::new("replace_messages", n),
            &messages,
            |b, msgs| {
                b.iter(|| {
                    let agent = new_agent();
                    agent.replace_messages(msgs.clone());
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_new_agent, bench_replace_messages);
criterion_main!(benches);
