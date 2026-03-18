//! Property-based tests for the SSE parser.
//!
//! Tests the `parse_sse_text` pure function extracted from the OpenAI Responses
//! provider.  Each property corresponds to an invariant from the task spec:
//!
//! - P1 (INV-1, INV-2): Any mix of valid/invalid/skipped SSE lines never
//!   panics, and the output event count equals the number of valid JSON data
//!   lines before `[DONE]`.
//! - P2: Non-data lines (comments, `event:`, empty) contribute zero events.
//! - P3: Malformed JSON after `data: ` is skipped — no panic, no error, just
//!   fewer events.

use ai::providers::openai_responses::parse_sse_text;
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Encode an integer as a trivially valid JSON object so the test doesn't
/// depend on any particular JSON shape.
fn json_event(i: usize) -> String {
    format!("{{\"seq\":{}}}", i)
}

/// Build an SSE stream that interleaves valid data lines, comment lines,
/// event: lines, and empty lines in the given proportions, always terminated
/// by `data: [DONE]`.  Returns `(text, expected_event_count)`.
fn build_stream(
    n_valid: usize,
    n_comments: usize,
    n_event_lines: usize,
    n_empty: usize,
) -> (String, usize) {
    let mut text = String::new();
    // Round-robin interleave
    let total = n_valid + n_comments + n_event_lines + n_empty;
    let mut vi = 0;
    let mut ci = 0;
    let mut ei = 0;
    let mut oi = 0;
    for idx in 0..total {
        let slot = idx % 4;
        if slot == 0 && vi < n_valid {
            text.push_str(&format!("data: {}\n", json_event(vi)));
            vi += 1;
        } else if slot == 1 && ci < n_comments {
            text.push_str(": this is a comment\n");
            ci += 1;
        } else if slot == 2 && ei < n_event_lines {
            text.push_str("event: someEvent\n");
            ei += 1;
        } else if oi < n_empty {
            text.push('\n');
            oi += 1;
        } else if vi < n_valid {
            text.push_str(&format!("data: {}\n", json_event(vi)));
            vi += 1;
        } else if ci < n_comments {
            text.push_str(": this is a comment\n");
            ci += 1;
        } else if ei < n_event_lines {
            text.push_str("event: someEvent\n");
            ei += 1;
        } else {
            text.push('\n');
        }
    }
    text.push_str("data: [DONE]\n");
    (text, n_valid)
}

// ---------------------------------------------------------------------------
// Strategy: arb_sse_stream
//
// Generates a complete SSE text body with a random mix of valid data lines,
// malformed data lines, comment lines, event: lines, empty lines, and a
// terminal [DONE].  Returns (text, expected_valid_count).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum SseLine {
    ValidData(usize), // data: {"seq":<n>}
    MalformedData,    // data: {NOT_JSON!!
    Comment,          // : comment
    EventLine,        // event: something
    Empty,            // (blank)
}

fn arb_sse_line() -> impl Strategy<Value = SseLine> {
    prop_oneof![
        3 => any::<usize>().prop_map(SseLine::ValidData),
        2 => Just(SseLine::MalformedData),
        1 => Just(SseLine::Comment),
        1 => Just(SseLine::EventLine),
        1 => Just(SseLine::Empty),
    ]
}

/// Returns `(sse_text, expected_valid_count)`.
fn arb_sse_stream() -> impl Strategy<Value = (String, usize)> {
    prop::collection::vec(arb_sse_line(), 0..30).prop_map(|lines| {
        let mut text = String::new();
        let mut valid_count = 0usize;
        for line in &lines {
            match line {
                SseLine::ValidData(n) => {
                    text.push_str(&format!("data: {}\n", json_event(*n)));
                    valid_count += 1;
                }
                SseLine::MalformedData => {
                    text.push_str("data: {NOT_JSON!!\n");
                }
                SseLine::Comment => {
                    text.push_str(": this is a comment\n");
                }
                SseLine::EventLine => {
                    text.push_str("event: someEvent\n");
                }
                SseLine::Empty => {
                    text.push('\n');
                }
            }
        }
        text.push_str("data: [DONE]\n");
        (text, valid_count)
    })
}

// ---------------------------------------------------------------------------
// P1 — Mixed stream: no panic, correct event count
// ---------------------------------------------------------------------------

proptest! {
    /// P1 (INV-1, INV-2): Any mixed SSE stream never panics and the output
    /// count equals the number of valid data lines before [DONE].
    #[test]
    fn proptest_p1_mixed_stream_no_panic_correct_count(
        (text, expected) in arb_sse_stream()
    ) {
        let events = parse_sse_text(&text);
        // INV-1: no panic (implicit — we reached this line)
        // INV-2: count matches valid JSON data-line count
        prop_assert_eq!(events.len(), expected);
    }
}

// ---------------------------------------------------------------------------
// P2 — Non-data lines are silently skipped
// ---------------------------------------------------------------------------

proptest! {
    /// P2: Inserting comment lines, `event:` lines, and empty lines between
    /// valid data lines does not change the output event count.
    #[test]
    fn proptest_p2_non_data_lines_are_skipped(
        n_valid in 0usize..15,
        n_comments in 0usize..10,
        n_event_lines in 0usize..10,
        n_empty in 0usize..10,
    ) {
        let (text, expected) = build_stream(n_valid, n_comments, n_event_lines, n_empty);
        let events = parse_sse_text(&text);
        prop_assert_eq!(events.len(), expected);
    }
}

// ---------------------------------------------------------------------------
// P3 — Malformed JSON is gracefully skipped
// ---------------------------------------------------------------------------

proptest! {
    /// P3: Malformed JSON after `data: ` is silently dropped — no panic, and
    /// the output contains only the events from well-formed data lines.
    #[test]
    fn proptest_p3_malformed_json_skipped_gracefully(
        n_valid in 0usize..15,
        n_malformed in 0usize..15,
    ) {
        let mut text = String::new();
        for i in 0..n_valid {
            text.push_str(&format!("data: {}\n", json_event(i)));
        }
        for _ in 0..n_malformed {
            text.push_str("data: {NOTJSON: definitely [ not ] valid}\n");
        }
        text.push_str("data: [DONE]\n");

        let events = parse_sse_text(&text);

        // INV-1: no panic
        // INV-2: only the well-formed lines produce events
        prop_assert_eq!(events.len(), n_valid);
    }
}

// ---------------------------------------------------------------------------
// P3b — Interleaved malformed + valid lines
// ---------------------------------------------------------------------------

proptest! {
    /// P3b: Malformed lines interleaved with valid lines — valid lines still
    /// produce events; malformed lines are silently dropped.
    #[test]
    fn proptest_p3b_interleaved_malformed_does_not_affect_valid(
        (text, expected) in arb_sse_stream()
    ) {
        // arb_sse_stream already mixes MalformedData with ValidData.
        // Re-assert here to make the property explicit.
        let events = parse_sse_text(&text);
        prop_assert_eq!(events.len(), expected,
            "valid event count mismatch in stream:\n{}", text);
    }
}
