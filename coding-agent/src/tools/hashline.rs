use anyhow::{bail, Result};

pub struct Anchor {
    pub line: usize,
    pub hash: String,
}

pub struct HashMismatch {
    pub line: usize,
    pub expected: String,
    pub actual: String,
}

const NIBBLE_STR: &[u8] = b"ZPMQVRWSNKTXJBYH";

// xxh32 constants
const XXH_PRIME32_1: u32 = 0x9E3779B1;
const XXH_PRIME32_2: u32 = 0x85EBCA77;
const XXH_PRIME32_3: u32 = 0xC2B2AE3D;
const XXH_PRIME32_4: u32 = 0x27D4EB2F;
const XXH_PRIME32_5: u32 = 0x165667B1;

/// Minimal xxHash32 implementation (single-shot, no streaming needed).
fn xxh32(input: &[u8], seed: u32) -> u32 {
    let len = input.len();
    let mut h: u32;
    let mut i = 0;

    if len >= 16 {
        let mut v1 = seed.wrapping_add(XXH_PRIME32_1).wrapping_add(XXH_PRIME32_2);
        let mut v2 = seed.wrapping_add(XXH_PRIME32_2);
        let mut v3 = seed;
        let mut v4 = seed.wrapping_sub(XXH_PRIME32_1);

        while i + 16 <= len {
            v1 = xxh32_round(v1, read32(input, i));
            v2 = xxh32_round(v2, read32(input, i + 4));
            v3 = xxh32_round(v3, read32(input, i + 8));
            v4 = xxh32_round(v4, read32(input, i + 12));
            i += 16;
        }

        h = v1
            .rotate_left(1)
            .wrapping_add(v2.rotate_left(7))
            .wrapping_add(v3.rotate_left(12))
            .wrapping_add(v4.rotate_left(18));
    } else {
        h = seed.wrapping_add(XXH_PRIME32_5);
    }

    h = h.wrapping_add(len as u32);

    // Remaining 4-byte chunks
    while i + 4 <= len {
        h = h.wrapping_add(read32(input, i).wrapping_mul(XXH_PRIME32_3));
        h = h.rotate_left(17).wrapping_mul(XXH_PRIME32_4);
        i += 4;
    }

    // Remaining bytes
    while i < len {
        h = h.wrapping_add((input[i] as u32).wrapping_mul(XXH_PRIME32_5));
        h = h.rotate_left(11).wrapping_mul(XXH_PRIME32_1);
        i += 1;
    }

    // Final avalanche
    h ^= h >> 15;
    h = h.wrapping_mul(XXH_PRIME32_2);
    h ^= h >> 13;
    h = h.wrapping_mul(XXH_PRIME32_3);
    h ^= h >> 16;
    h
}

#[inline]
fn xxh32_round(acc: u32, input: u32) -> u32 {
    acc.wrapping_add(input.wrapping_mul(XXH_PRIME32_2))
        .rotate_left(13)
        .wrapping_mul(XXH_PRIME32_1)
}

#[inline]
fn read32(buf: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]])
}

pub fn compute_line_hash(line_number: usize, line: &str) -> String {
    let stripped = line.strip_suffix('\r').unwrap_or(line);
    let mut normalized = stripped.to_string();
    normalized.retain(|c| !c.is_whitespace());

    let seed = if !normalized.chars().any(|c| c.is_alphanumeric()) {
        line_number as u32
    } else {
        0
    };

    let byte = (xxh32(normalized.as_bytes(), seed) & 0xFF) as u8;
    let hi = NIBBLE_STR[(byte >> 4) as usize] as char;
    let lo = NIBBLE_STR[(byte & 0x0F) as usize] as char;
    format!("{hi}{lo}")
}

pub fn format_hash_lines(text: &str, start_line: usize) -> String {
    text.lines()
        .enumerate()
        .map(|(i, line)| {
            let num = start_line + i;
            let hash = compute_line_hash(num, line);
            format!("{num}#{hash}:{line}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_nibble_char(c: u8) -> bool {
    NIBBLE_STR.contains(&c)
}

pub fn parse_tag(ref_str: &str) -> Result<Anchor> {
    let bytes = ref_str.as_bytes();
    let mut pos = 0;

    // Skip leading '>', '+', '-' characters and whitespace
    while pos < bytes.len() {
        let b = bytes[pos];
        if b == b'>' || b == b'+' || b == b'-' || (b as char).is_whitespace() {
            pos += 1;
        } else {
            break;
        }
    }

    // Parse digits (must have at least one)
    let digit_start = pos;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }
    if pos == digit_start {
        bail!("parse_tag: no digits found in {:?}", ref_str);
    }
    let line: usize = std::str::from_utf8(&bytes[digit_start..pos])
        .unwrap()
        .parse()
        .map_err(|e| anyhow::anyhow!("parse_tag: invalid line number: {e}"))?;

    // Expect '#'
    if pos >= bytes.len() || bytes[pos] != b'#' {
        bail!("parse_tag: expected '#' after line number in {:?}", ref_str);
    }
    pos += 1;

    // Parse exactly 2 NIBBLE_STR chars
    if pos + 2 > bytes.len() {
        bail!(
            "parse_tag: expected 2 hash chars after '#' in {:?}",
            ref_str
        );
    }
    let c1 = bytes[pos];
    let c2 = bytes[pos + 1];
    if !is_nibble_char(c1) || !is_nibble_char(c2) {
        bail!(
            "parse_tag: invalid hash chars '{}{}' in {:?}",
            c1 as char,
            c2 as char,
            ref_str
        );
    }

    let hash = format!("{}{}", c1 as char, c2 as char);
    Ok(Anchor { line, hash })
}

pub fn validate_all_refs(refs: &[&Anchor], lines: &[&str]) -> Result<(), Vec<HashMismatch>> {
    let mut mismatches = Vec::new();

    for anchor in refs {
        if anchor.line < 1 || anchor.line > lines.len() {
            mismatches.push(HashMismatch {
                line: anchor.line,
                expected: anchor.hash.clone(),
                actual: String::from("OUT_OF_RANGE"),
            });
            continue;
        }
        let actual = compute_line_hash(anchor.line, lines[anchor.line - 1]);
        if actual != anchor.hash {
            mismatches.push(HashMismatch {
                line: anchor.line,
                expected: anchor.hash.clone(),
                actual,
            });
        }
    }

    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(mismatches)
    }
}

pub fn format_mismatch_error(mismatches: &[HashMismatch], lines: &[&str]) -> String {
    let mut out = String::new();
    for mm in mismatches {
        out.push_str(&format!(
            "Hash mismatch at line {} — expected {} but file has {}:\n",
            mm.line, mm.expected, mm.actual
        ));

        // Show 2 lines of context before
        let start = if mm.line > 2 { mm.line - 2 } else { 1 };
        let end = if mm.line + 2 <= lines.len() {
            mm.line + 2
        } else {
            lines.len()
        };

        for ln in start..=end {
            if ln < 1 || ln > lines.len() {
                continue;
            }
            let content = lines[ln - 1];
            if ln == mm.line {
                let hash = compute_line_hash(ln, content);
                out.push_str(&format!(">>> {ln}#{hash}:{content}\n"));
            } else {
                out.push_str(&format!("  {ln}  {content}\n"));
            }
        }

        out.push_str("Re-read the file for updated hashes.\n");
    }
    out
}

pub fn strip_hash_prefixes(lines: &[String]) -> Vec<String> {
    // Check if ALL non-empty lines match the pattern: digits # 2-nibble-chars :
    let all_match = lines.iter().all(|line| {
        if line.is_empty() {
            return true; // empty lines pass through
        }
        has_hash_prefix(line)
    });

    if all_match {
        lines
            .iter()
            .map(|line| {
                if line.is_empty() {
                    return String::new();
                }
                // Find the ':' after the prefix and return everything after it
                if let Some(colon_pos) = find_prefix_colon(line) {
                    line[colon_pos + 1..].to_string()
                } else {
                    line.clone()
                }
            })
            .collect()
    } else {
        lines.to_vec()
    }
}

/// Check if a line has the hash prefix pattern: digits # 2-nibble-chars :
fn has_hash_prefix(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut pos = 0;

    // Parse digits (at least one)
    let digit_start = pos;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }
    if pos == digit_start {
        return false;
    }

    // Expect '#'
    if pos >= bytes.len() || bytes[pos] != b'#' {
        return false;
    }
    pos += 1;

    // Expect exactly 2 NIBBLE_STR chars
    if pos + 2 > bytes.len() {
        return false;
    }
    if !is_nibble_char(bytes[pos]) || !is_nibble_char(bytes[pos + 1]) {
        return false;
    }
    pos += 2;

    // Expect ':'
    if pos >= bytes.len() || bytes[pos] != b':' {
        return false;
    }

    true
}

/// Find the position of ':' after the hash prefix
fn find_prefix_colon(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut pos = 0;

    // Skip digits
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }
    // Skip '#'
    pos += 1;
    // Skip 2 nibble chars
    pos += 2;
    // Should be ':'
    if pos < bytes.len() && bytes[pos] == b':' {
        Some(pos)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_line_hash_is_deterministic() {
        let h1 = compute_line_hash(1, "let x = 42;");
        let h2 = compute_line_hash(1, "let x = 42;");
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_output_is_two_nibble_chars() {
        let h = compute_line_hash(1, "fn main() {}");
        assert_eq!(h.len(), 2);
        for c in h.bytes() {
            assert!(
                NIBBLE_STR.contains(&c),
                "char '{}' not in NIBBLE_STR",
                c as char
            );
        }
    }

    #[test]
    fn symbol_only_lines_differ_by_line_number() {
        // Symbol-only lines use line_number as seed, so different line numbers → different hashes
        let h1 = compute_line_hash(1, "}");
        let h2 = compute_line_hash(2, "}");
        let h3 = compute_line_hash(3, "}");
        // At least two of these should differ (extremely likely all three differ)
        assert!(
            h1 != h2 || h2 != h3,
            "expected symbol-only lines at different line numbers to produce different hashes"
        );

        let ha = compute_line_hash(1, "{");
        let hb = compute_line_hash(5, "{");
        assert_ne!(ha, hb);

        let hc = compute_line_hash(10, "***");
        let hd = compute_line_hash(20, "***");
        assert_ne!(hc, hd);
    }

    #[test]
    fn alphanumeric_lines_same_hash_regardless_of_line_number() {
        let h1 = compute_line_hash(1, "let x = 42;");
        let h2 = compute_line_hash(100, "let x = 42;");
        assert_eq!(h1, h2, "alphanumeric lines should use seed=0");
    }

    #[test]
    fn parse_tag_valid_basic() {
        let anchor = parse_tag("5#ZP").unwrap();
        assert_eq!(anchor.line, 5);
        assert_eq!(anchor.hash, "ZP");
    }

    #[test]
    fn parse_tag_valid_with_whitespace() {
        let anchor = parse_tag("  5#ZP").unwrap();
        assert_eq!(anchor.line, 5);
        assert_eq!(anchor.hash, "ZP");
    }

    #[test]
    fn parse_tag_valid_with_prefix() {
        let anchor = parse_tag(">>>5#ZP").unwrap();
        assert_eq!(anchor.line, 5);
        assert_eq!(anchor.hash, "ZP");
    }

    #[test]
    fn parse_tag_invalid_wrong_alphabet() {
        assert!(parse_tag("5#AA").is_err());
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
    fn format_hash_lines_round_trip() {
        let text = "fn main() {\n    let x = 1;\n    println!(\"{}\", x);\n}";
        let formatted = format_hash_lines(text, 1);

        let lines: Vec<&str> = text.lines().collect();

        for formatted_line in formatted.lines() {
            let anchor = parse_tag(formatted_line).unwrap();
            let expected_hash = compute_line_hash(anchor.line, lines[anchor.line - 1]);
            assert_eq!(anchor.hash, expected_hash);
        }
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
    fn strip_hash_prefixes_mixed_returns_unchanged() {
        let lines = vec![
            "1#ZP:fn main() {".to_string(),
            "    let x = 1;".to_string(), // no prefix
            "3#VR:}".to_string(),
        ];
        let result = strip_hash_prefixes(&lines);
        assert_eq!(result, lines);
    }

    #[test]
    fn strip_hash_prefixes_none_prefixed_returns_unchanged() {
        let lines = vec![
            "fn main() {".to_string(),
            "    let x = 1;".to_string(),
            "}".to_string(),
        ];
        let result = strip_hash_prefixes(&lines);
        assert_eq!(result, lines);
    }
}
