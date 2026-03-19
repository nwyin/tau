use coding_agent::tools::hashline::*;

#[test]
fn compute_line_hash_determinism() {
    let h1 = compute_line_hash(1, "hello world");
    let h2 = compute_line_hash(1, "hello world");
    assert_eq!(h1, h2);
}

#[test]
fn compute_line_hash_two_char_nibble_output() {
    let h = compute_line_hash(1, "fn main() {}");
    assert_eq!(h.len(), 2);
    let nibble_chars: &[u8] = b"ZPMQVRWSNKTXJBYH";
    for b in h.bytes() {
        assert!(nibble_chars.contains(&b), "unexpected char '{}'", b as char);
    }
}

#[test]
fn symbol_only_lines_use_line_number_seed() {
    // Same content at different line numbers should produce different hashes
    let h1 = compute_line_hash(1, "}");
    let h2 = compute_line_hash(2, "}");
    assert_ne!(
        h1, h2,
        "symbol-only lines at different positions should differ"
    );

    let h3 = compute_line_hash(10, "{");
    let h4 = compute_line_hash(20, "{");
    assert_ne!(h3, h4);
}

#[test]
fn alphanumeric_lines_ignore_line_number() {
    let h1 = compute_line_hash(1, "let x = 42;");
    let h2 = compute_line_hash(999, "let x = 42;");
    assert_eq!(
        h1, h2,
        "alphanumeric lines should use seed=0, ignoring line number"
    );
}

#[test]
fn parse_tag_valid_basic() {
    let a = parse_tag("5#ZP").unwrap();
    assert_eq!(a.line, 5);
    assert_eq!(a.hash, "ZP");
}

#[test]
fn parse_tag_valid_with_leading_whitespace() {
    let a = parse_tag("  42#MQ").unwrap();
    assert_eq!(a.line, 42);
    assert_eq!(a.hash, "MQ");
}

#[test]
fn parse_tag_valid_with_arrow_prefix() {
    let a = parse_tag(">>>10#VR").unwrap();
    assert_eq!(a.line, 10);
    assert_eq!(a.hash, "VR");
}

#[test]
fn parse_tag_valid_with_diff_markers() {
    let a = parse_tag("+5#ZP").unwrap();
    assert_eq!(a.line, 5);
    let b = parse_tag("-5#ZP").unwrap();
    assert_eq!(b.line, 5);
}

#[test]
fn parse_tag_invalid_wrong_alphabet() {
    assert!(parse_tag("5#AA").is_err());
    assert!(parse_tag("5#ab").is_err());
}

#[test]
fn parse_tag_invalid_missing_hash() {
    assert!(parse_tag("5ZP").is_err());
}

#[test]
fn parse_tag_invalid_no_digits() {
    assert!(parse_tag("#ZP").is_err());
}

#[test]
fn format_hash_lines_round_trips_with_parse_and_validate() {
    let text = "fn main() {\n    let x = 1;\n    println!(\"{}\", x);\n}";
    let formatted = format_hash_lines(text, 1);
    let original_lines: Vec<&str> = text.lines().collect();

    for formatted_line in formatted.lines() {
        let anchor = parse_tag(formatted_line).unwrap();
        let expected_hash = compute_line_hash(anchor.line, original_lines[anchor.line - 1]);
        assert_eq!(anchor.hash, expected_hash);
    }

    // Also validate via validate_all_refs
    let anchors: Vec<Anchor> = formatted.lines().map(|l| parse_tag(l).unwrap()).collect();
    let anchor_refs: Vec<&Anchor> = anchors.iter().collect();
    assert!(validate_all_refs(&anchor_refs, &original_lines).is_ok());
}

#[test]
fn format_hash_lines_with_offset() {
    let text = "line a\nline b";
    let formatted = format_hash_lines(text, 5);
    let lines: Vec<&str> = formatted.lines().collect();
    assert!(lines[0].starts_with("5#"));
    assert!(lines[1].starts_with("6#"));
}

#[test]
fn validate_all_refs_detects_mismatch() {
    let lines = vec!["hello", "world"];
    let good = Anchor {
        line: 1,
        hash: compute_line_hash(1, "hello"),
    };
    let bad = Anchor {
        line: 2,
        hash: "ZZ".to_string(),
    }; // wrong hash
    let refs = vec![&good, &bad];
    let result = validate_all_refs(&refs, &lines);
    assert!(result.is_err());
    let mismatches = result.unwrap_err();
    assert_eq!(mismatches.len(), 1);
    assert_eq!(mismatches[0].line, 2);
}

#[test]
fn validate_all_refs_out_of_range() {
    let lines = vec!["hello"];
    let anchor = Anchor {
        line: 5,
        hash: "ZP".to_string(),
    };
    let refs = vec![&anchor];
    let result = validate_all_refs(&refs, &lines);
    assert!(result.is_err());
    let mismatches = result.unwrap_err();
    assert_eq!(mismatches[0].actual, "OUT_OF_RANGE");
}

#[test]
fn format_mismatch_error_includes_context() {
    let lines = vec!["line 1", "line 2", "line 3", "line 4", "line 5"];
    let mm = HashMismatch {
        line: 3,
        expected: "ZZ".to_string(),
        actual: compute_line_hash(3, "line 3"),
    };
    let output = format_mismatch_error(&[mm], &lines);
    assert!(output.contains("Hash mismatch at line 3"));
    assert!(output.contains(">>>"));
    assert!(output.contains("Re-read the file"));
}

#[test]
fn strip_hash_prefixes_all_prefixed() {
    let lines = vec![
        "1#ZP:fn main() {".to_string(),
        "2#MQ:    let x = 1;".to_string(),
        "3#VR:}".to_string(),
    ];
    let stripped = strip_hash_prefixes(&lines);
    assert_eq!(stripped[0], "fn main() {");
    assert_eq!(stripped[1], "    let x = 1;");
    assert_eq!(stripped[2], "}");
}

#[test]
fn strip_hash_prefixes_with_empty_lines() {
    let lines = vec![
        "1#ZP:hello".to_string(),
        "".to_string(),
        "3#VR:world".to_string(),
    ];
    let stripped = strip_hash_prefixes(&lines);
    assert_eq!(stripped[0], "hello");
    assert_eq!(stripped[1], "");
    assert_eq!(stripped[2], "world");
}

#[test]
fn strip_hash_prefixes_mixed_returns_unchanged() {
    let lines = vec![
        "1#ZP:hello".to_string(),
        "no prefix here".to_string(),
        "3#VR:world".to_string(),
    ];
    let result = strip_hash_prefixes(&lines);
    assert_eq!(result, lines);
}

#[test]
fn strip_hash_prefixes_none_prefixed() {
    let lines = vec!["fn main() {".to_string(), "    let x = 1;".to_string()];
    let result = strip_hash_prefixes(&lines);
    assert_eq!(result, lines);
}
