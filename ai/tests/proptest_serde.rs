//! Property-based serde round-trip tests.
//!
//! Invariants tested (INV-3 through INV-5):
//!
//! - INV-3: All ContentBlock variants round-trip faithfully through
//!   serde_json (P4).
//! - INV-4: All Message variants round-trip faithfully through serde_json
//!   (P5, P6).
//! - INV-5: StopReason round-trips through JSON string serialization (P7).
//!
//! Because ContentBlock and Message do not derive PartialEq, the round-trip
//! is verified by serialising twice: `to_value(x) == to_value(from_value(to_value(x)))`.
//! For StopReason (which does derive PartialEq) we compare values directly.

use ai::types::{
    AssistantMessage, ContentBlock, Cost, Message, StopReason, ToolResultMessage, Usage, UserBlock,
    UserContent, UserMessage,
};
use proptest::prelude::*;
use proptest::strategy::BoxedStrategy;
use serde_json::Value;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Verify that a type round-trips: serialize → deserialize → serialize gives
/// the same JSON value.  Works for any Serialize + DeserializeOwned type.
fn assert_json_roundtrip<T>(val: &T) -> Result<(), TestCaseError>
where
    T: serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug,
{
    let json1 = serde_json::to_value(val)
        .map_err(|e| TestCaseError::fail(format!("first serialize failed: {e}")))?;
    let restored: T = serde_json::from_value(json1.clone())
        .map_err(|e| TestCaseError::fail(format!("deserialize failed: {e}\nJSON: {json1}")))?;
    let json2 = serde_json::to_value(&restored)
        .map_err(|e| TestCaseError::fail(format!("second serialize failed: {e}")))?;
    prop_assert_eq!(&json1, &json2, "JSON values differ after round-trip");
    Ok(())
}

// ---------------------------------------------------------------------------
// Leaf value strategy (non-recursive JSON)
// ---------------------------------------------------------------------------

fn arb_leaf_value() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        // Use i32 to stay well within JSON integer range
        any::<i32>().prop_map(|n| serde_json::json!(n)),
        // Bounded printable ASCII string
        "[\\x20-\\x7e]{0,50}".prop_map(Value::String),
    ]
}

/// Bounded JSON value (max depth 2): leaf | array-of-leaves | object-of-leaves.
fn arb_json_value() -> impl Strategy<Value = Value> {
    prop_oneof![
        4 => arb_leaf_value(),
        1 => prop::collection::vec(arb_leaf_value(), 0..5).prop_map(Value::Array),
        1 => prop::collection::hash_map("[a-z]{1,8}", arb_leaf_value(), 0..5)
                .prop_map(|m| Value::Object(m.into_iter().collect())),
    ]
}

// ---------------------------------------------------------------------------
// String strategies
// ---------------------------------------------------------------------------

/// Printable ASCII string, bounded length.
fn arb_ascii(max: usize) -> BoxedStrategy<String> {
    proptest::string::string_regex(&format!("[\\x20-\\x7e]{{0,{max}}}"))
        .expect("valid ascii regex")
        .boxed()
}

/// Bounded Unicode string (arbitrary chars, capped at `max_chars`).
fn arb_unicode(max_chars: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(any::<char>(), 0..=max_chars)
        .prop_map(|chars| chars.into_iter().collect::<String>())
}

/// Mixed strategy — either printable ASCII or arbitrary Unicode.
fn arb_text(max_len: usize) -> BoxedStrategy<String> {
    prop_oneof![arb_ascii(max_len), arb_unicode(max_len),].boxed()
}

// ---------------------------------------------------------------------------
// arb_content_block
// ---------------------------------------------------------------------------

fn arb_tool_args() -> impl Strategy<Value = HashMap<String, Value>> {
    prop::collection::hash_map("[a-z]{1,10}", arb_json_value(), 0..10)
}

fn arb_content_block() -> impl Strategy<Value = ContentBlock> {
    prop_oneof![
        // Text variant
        (arb_text(500), prop::option::of(arb_ascii(100))).prop_map(|(text, text_signature)| {
            ContentBlock::Text {
                text,
                text_signature,
            }
        }),
        // Thinking variant (with optional signature and optional redacted flag)
        (
            arb_text(500),
            prop::option::of(arb_ascii(100)),
            prop::option::of(any::<bool>()),
        )
            .prop_map(
                |(thinking, thinking_signature, redacted)| ContentBlock::Thinking {
                    thinking,
                    thinking_signature,
                    redacted,
                }
            ),
        // Image variant (base64-ish data + one of four common MIME types)
        (
            "[A-Za-z0-9+/=]{0,120}",
            prop_oneof![
                Just("image/jpeg".to_string()),
                Just("image/png".to_string()),
                Just("image/gif".to_string()),
                Just("image/webp".to_string()),
            ],
        )
            .prop_map(|(data, mime_type)| ContentBlock::Image { data, mime_type }),
        // ToolCall variant (deeply-nested args covered by arb_json_value)
        (
            "[a-zA-Z0-9_-]{1,50}",
            "[a-zA-Z_][a-zA-Z0-9_]{0,49}",
            arb_tool_args(),
            prop::option::of(arb_ascii(100)),
        )
            .prop_map(
                |(id, name, arguments, thought_signature)| ContentBlock::ToolCall {
                    id,
                    name,
                    arguments,
                    thought_signature,
                }
            ),
    ]
}

// ---------------------------------------------------------------------------
// arb_user_block / arb_user_content
// ---------------------------------------------------------------------------

fn arb_user_block() -> impl Strategy<Value = UserBlock> {
    prop_oneof![
        arb_text(200).prop_map(|text| UserBlock::Text { text }),
        (
            "[A-Za-z0-9+/=]{0,80}",
            prop_oneof![
                Just("image/jpeg".to_string()),
                Just("image/png".to_string()),
            ],
        )
            .prop_map(|(data, mime_type)| UserBlock::Image { data, mime_type }),
    ]
}

fn arb_user_content() -> impl Strategy<Value = UserContent> {
    prop_oneof![
        // UserContent::Text serialises as a plain JSON string.
        arb_text(300).prop_map(UserContent::Text),
        // UserContent::Blocks serialises as a JSON array.
        prop::collection::vec(arb_user_block(), 0..5).prop_map(UserContent::Blocks),
    ]
}

// ---------------------------------------------------------------------------
// arb_stop_reason
// ---------------------------------------------------------------------------

fn arb_stop_reason() -> impl Strategy<Value = StopReason> {
    prop_oneof![
        Just(StopReason::Stop),
        Just(StopReason::Length),
        Just(StopReason::ToolUse),
        Just(StopReason::Error),
        Just(StopReason::Aborted),
    ]
}

// ---------------------------------------------------------------------------
// arb_message
// ---------------------------------------------------------------------------

fn arb_user_message() -> impl Strategy<Value = Message> {
    (arb_user_content(), any::<i64>()).prop_map(|(content, timestamp)| {
        Message::User(UserMessage {
            role: "user".into(),
            content,
            timestamp,
        })
    })
}

fn arb_assistant_message() -> impl Strategy<Value = Message> {
    (
        prop::collection::vec(arb_content_block(), 0..4),
        arb_stop_reason(),
        any::<i64>(),
        prop::option::of(arb_ascii(100)),
    )
        .prop_map(|(content, stop_reason, timestamp, error_message)| {
            Message::Assistant(AssistantMessage {
                role: "assistant".into(),
                content,
                api: "openai-responses".into(),
                provider: "openai".into(),
                model: "gpt-4o".into(),
                usage: Usage {
                    cost: Cost::default(),
                    ..Usage::default()
                },
                stop_reason,
                error_message,
                timestamp,
            })
        })
}

fn arb_tool_result_message() -> impl Strategy<Value = Message> {
    (
        "[a-zA-Z0-9_-]{1,50}",
        "[a-zA-Z_][a-zA-Z0-9_]{0,49}",
        prop::collection::vec(arb_user_block(), 0..4),
        any::<bool>(),
        any::<i64>(),
    )
        .prop_map(|(tool_call_id, tool_name, content, is_error, timestamp)| {
            Message::ToolResult(ToolResultMessage {
                role: "toolResult".into(),
                tool_call_id,
                tool_name,
                content,
                details: None,
                is_error,
                timestamp,
            })
        })
}

fn arb_message() -> impl Strategy<Value = Message> {
    prop_oneof![
        arb_user_message(),
        arb_assistant_message(),
        arb_tool_result_message(),
    ]
}

// ---------------------------------------------------------------------------
// P4 — ContentBlock round-trips (INV-3)
// ---------------------------------------------------------------------------

proptest! {
    /// P4 (INV-3): Every ContentBlock variant serialises and deserialises
    /// without loss — JSON value before and after the round-trip is identical.
    #[test]
    fn proptest_p4_content_block_roundtrips(block in arb_content_block()) {
        assert_json_roundtrip(&block)?;
    }

    /// P4b: ContentBlock::Text with a None text_signature correctly omits the
    /// field in JSON (skip_serializing_if) and deserialises back to None.
    #[test]
    fn proptest_p4b_text_none_signature_omitted(text in arb_text(200)) {
        let block = ContentBlock::Text { text, text_signature: None };
        let json = serde_json::to_value(&block).unwrap();
        prop_assert!(!json.as_object().unwrap().contains_key("textSignature"),
            "textSignature should be absent when None");
        assert_json_roundtrip(&block)?;
    }

    /// P4c: ContentBlock::Thinking with optional fields — None → absent in
    /// JSON, Some(x) → present.
    #[test]
    fn proptest_p4c_thinking_optional_fields(
        thinking in arb_text(200),
        sig in prop::option::of(arb_ascii(80)),
        redacted in prop::option::of(any::<bool>()),
    ) {
        let block = ContentBlock::Thinking { thinking, thinking_signature: sig.clone(), redacted };
        let json = serde_json::to_value(&block).unwrap();
        let obj = json.as_object().unwrap();
        if sig.is_none() {
            prop_assert!(!obj.contains_key("thinkingSignature"));
        } else {
            prop_assert!(obj.contains_key("thinkingSignature"));
        }
        assert_json_roundtrip(&block)?;
    }

    /// P4d: ToolCall with empty arguments HashMap — serialises and deserialises
    /// correctly (empty object `{}`).
    #[test]
    fn proptest_p4d_tool_call_empty_args(
        id in "[a-zA-Z0-9_-]{1,30}",
        name in "[a-zA-Z_][a-zA-Z0-9_]{0,29}",
    ) {
        let block = ContentBlock::ToolCall {
            id,
            name,
            arguments: HashMap::new(),
            thought_signature: None,
        };
        let json = serde_json::to_value(&block).unwrap();
        let args = &json["arguments"];
        prop_assert!(args.is_object() && args.as_object().unwrap().is_empty(),
            "empty arguments should serialise as {{}}");
        assert_json_roundtrip(&block)?;
    }

    /// P4e: ToolCall with nested JSON values in arguments (depth-2 objects).
    #[test]
    fn proptest_p4e_tool_call_nested_args(args in arb_tool_args()) {
        let block = ContentBlock::ToolCall {
            id: "test-id".into(),
            name: "test_fn".into(),
            arguments: args,
            thought_signature: None,
        };
        assert_json_roundtrip(&block)?;
    }
}

// ---------------------------------------------------------------------------
// P5 — Message round-trips (INV-4)
// ---------------------------------------------------------------------------

proptest! {
    /// P5 (INV-4): Every Message variant serialises and deserialises without
    /// loss.
    #[test]
    fn proptest_p5_message_roundtrips(msg in arb_message()) {
        assert_json_roundtrip(&msg)?;
    }
}

// ---------------------------------------------------------------------------
// P6 — UserContent untagged union disambiguation (INV-4)
// ---------------------------------------------------------------------------

proptest! {
    /// P6a: UserContent::Text(s) serialises as a plain JSON string and
    /// deserialises back to UserContent::Text.
    #[test]
    fn proptest_p6a_user_content_text_is_string(s in arb_text(300)) {
        let content = UserContent::Text(s.clone());
        let json = serde_json::to_value(&content).unwrap();
        prop_assert!(json.is_string(), "UserContent::Text should serialise as a JSON string");
        prop_assert_eq!(json.as_str().unwrap(), s.as_str());
        // round-trip
        let restored: UserContent = serde_json::from_value(json).unwrap();
        let json2 = serde_json::to_value(&restored).unwrap();
        prop_assert_eq!(json2.as_str().unwrap(), s.as_str());
    }

    /// P6b: UserContent::Blocks(v) serialises as a JSON array and
    /// deserialises back to UserContent::Blocks.
    #[test]
    fn proptest_p6b_user_content_blocks_is_array(
        blocks in prop::collection::vec(arb_user_block(), 0..5)
    ) {
        let content = UserContent::Blocks(blocks.clone());
        let json = serde_json::to_value(&content).unwrap();
        prop_assert!(json.is_array(), "UserContent::Blocks should serialise as a JSON array");
        prop_assert_eq!(json.as_array().unwrap().len(), blocks.len());
        assert_json_roundtrip(&content)?;
    }
}

// ---------------------------------------------------------------------------
// P7 — StopReason round-trips (INV-5)
// ---------------------------------------------------------------------------

proptest! {
    /// P7 (INV-5): Every StopReason variant round-trips through JSON string
    /// serialisation and preserves the correct camelCase rename rules.
    #[test]
    fn proptest_p7_stop_reason_roundtrips(reason in arb_stop_reason()) {
        let json = serde_json::to_value(&reason).unwrap();
        // StopReason serialises as a plain JSON string
        prop_assert!(json.is_string(),
            "StopReason should serialise as a JSON string, got: {json}");
        let restored: StopReason = serde_json::from_value(json.clone()).unwrap();
        prop_assert_eq!(reason, restored);
    }

    /// P7b: Specific rename rules — verify the exact JSON strings produced.
    #[test]
    fn proptest_p7b_stop_reason_rename_rules(_dummy in Just(())) {
        let cases: &[(StopReason, &str)] = &[
            (StopReason::Stop, "stop"),
            (StopReason::Length, "length"),
            (StopReason::ToolUse, "toolUse"),
            (StopReason::Error, "error"),
            (StopReason::Aborted, "aborted"),
        ];
        for (variant, expected_str) in cases {
            let json = serde_json::to_value(variant).unwrap();
            prop_assert_eq!(json.as_str().unwrap(), *expected_str,
                "{:?} should serialise as \"{}\"", variant, expected_str);
        }
    }
}
