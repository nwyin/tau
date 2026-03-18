use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use ai::providers::openai_responses::parse_sse_text;

fn build_bench_stream(n_events: usize) -> String {
    let mut text = String::new();
    for i in 0..n_events {
        if i % 10 == 0 {
            text.push_str(": keepalive\n\n");
        }
        text.push_str(&format!(
            "data: {{\"type\":\"response.output_text.delta\",\"delta\":\"word{}\"}}\n\n",
            i
        ));
    }
    text.push_str("data: [DONE]\n");
    text
}

fn bench_sse_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("sse_parsing");

    for &n in &[10usize, 100, 1000] {
        let stream = build_bench_stream(n);
        group.bench_with_input(BenchmarkId::new("events", n), &stream, |b, s| {
            b.iter(|| parse_sse_text(s))
        });
    }

    group.finish();
}

criterion_group!(benches, bench_sse_parsing);
criterion_main!(benches);
