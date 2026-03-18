use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use ai::types::{
    AssistantMessage, ContentBlock, Cost, Message, StopReason, ToolResultMessage, Usage,
    UserBlock, UserContent, UserMessage,
};

/// Build a deterministic mix of Messages of the given count.
/// Pattern: user, assistant (1-3 text blocks), tool_result, repeat.
fn build_messages(n: usize) -> Vec<Message> {
    let mut msgs = Vec::with_capacity(n);
    let texts = [
        "Hello, how are you doing today? I have a question about Rust async patterns.",
        "Sure, I can help you with that. Rust's async system is built on futures and executors.",
        "The tokio runtime provides async I/O, timers, and task scheduling for concurrent work.",
        "Could you explain the difference between async fn and returning impl Future?",
        "An async fn desugars into a state machine implementing Future automatically by the compiler.",
        "What about Send bounds? My future needs to cross thread boundaries safely.",
        "Futures that capture non-Send types are not Send themselves — wrap in Arc<Mutex<T>>.",
        "I see, thanks! One more question: when should I use spawn_blocking vs just await?",
        "Use spawn_blocking for CPU-intensive or blocking I/O that would block the executor thread.",
        "That makes sense. I'll refactor my file reading code to use spawn_blocking then.",
    ];

    for i in 0..n {
        let slot = i % 3;
        match slot {
            0 => {
                let text = texts[i % texts.len()];
                let extra = "a".repeat((i % 150) + 50); // 50–199 extra chars
                msgs.push(Message::User(UserMessage {
                    role: "user".into(),
                    content: UserContent::Text(format!("{} {}", text, extra)),
                    timestamp: 1_700_000_000_000 + i as i64 * 1000,
                }));
            }
            1 => {
                let block_count = (i % 3) + 1;
                let mut content = Vec::with_capacity(block_count);
                for b in 0..block_count {
                    content.push(ContentBlock::Text {
                        text: format!("Response paragraph {} for turn {}.", b + 1, i),
                        text_signature: None,
                    });
                }
                msgs.push(Message::Assistant(AssistantMessage {
                    role: "assistant".into(),
                    content,
                    api: "anthropic-messages".into(),
                    provider: "anthropic".into(),
                    model: "claude-sonnet-4-5".into(),
                    usage: Usage {
                        input: 1200 + i as u64 * 7,
                        output: 300 + i as u64 * 3,
                        cache_read: 0,
                        cache_write: 0,
                        total_tokens: 1500 + i as u64 * 10,
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
                }));
            }
            _ => {
                msgs.push(Message::ToolResult(ToolResultMessage {
                    role: "toolResult".into(),
                    tool_call_id: format!("call_{:04x}", i),
                    tool_name: ["read_file", "write_file", "bash", "search"][i % 4].into(),
                    content: vec![UserBlock::Text {
                        text: format!("Tool output for call {} at turn {}.", i, i),
                    }],
                    details: None,
                    is_error: false,
                    timestamp: 1_700_000_001_000 + i as i64 * 1000,
                }));
            }
        }
    }
    msgs
}

fn bench_serialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_serde");
    for n in [10usize, 50, 100] {
        let messages = build_messages(n);
        group.bench_with_input(BenchmarkId::new("serialize", n), &messages, |b, msgs| {
            b.iter(|| serde_json::to_vec(msgs).expect("serialize ok"));
        });
    }
    group.finish();
}

fn bench_deserialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_serde");
    for n in [10usize, 50, 100] {
        let messages = build_messages(n);
        let bytes = serde_json::to_vec(&messages).expect("serialize ok");
        group.bench_with_input(BenchmarkId::new("deserialize", n), &bytes, |b, data| {
            b.iter(|| serde_json::from_slice::<Vec<Message>>(data).expect("deserialize ok"));
        });
    }
    group.finish();
}

fn bench_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_serde");
    for n in [10usize, 50, 100] {
        let messages = build_messages(n);
        group.bench_with_input(BenchmarkId::new("roundtrip", n), &messages, |b, msgs| {
            b.iter(|| {
                let bytes = serde_json::to_vec(msgs).expect("serialize ok");
                serde_json::from_slice::<Vec<Message>>(&bytes).expect("deserialize ok")
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_serialize, bench_deserialize, bench_roundtrip);
criterion_main!(benches);
