//! Mechanical context compaction — no LLM required.
//!
//! Three tiers of reduction applied in order:
//!   Tier 1 — truncate large tool outputs (≥50KB or ≥2000 lines) to 40%+40%
//!   Tier 2 — mask old turns: replace tool-result content with omission placeholders
//!   Fallback — aggressively truncate remaining large tool outputs (20%+20%)

use ai::types::{ContentBlock, Message, Model, UserBlock, UserContent};

use crate::types::AgentMessage;

const CHARS_PER_TOKEN: usize = 4;
const IMAGE_TOKEN_ESTIMATE: usize = 1200;
const BUDGET_FACTOR: f64 = 0.75;
const SYSTEM_PROMPT_ESTIMATE: usize = 2000; // tokens
const MAX_TOOL_OUTPUT_CHARS: usize = 50_000; // 50 KB
const MAX_TOOL_OUTPUT_LINES: usize = 2000;

// ---------------------------------------------------------------------------
// Token estimation
// ---------------------------------------------------------------------------

/// Estimate the total token count for a message slice (chars / 4).
pub fn estimate_tokens(messages: &[AgentMessage]) -> usize {
    messages.iter().map(chars_for_message).sum::<usize>() / CHARS_PER_TOKEN
}

fn chars_for_message(msg: &AgentMessage) -> usize {
    match msg {
        AgentMessage::Llm(Message::User(um)) => match &um.content {
            UserContent::Text(s) => s.len(),
            UserContent::Blocks(blocks) => blocks.iter().map(chars_for_user_block).sum(),
        },
        AgentMessage::Llm(Message::Assistant(am)) => {
            am.content.iter().map(chars_for_content_block).sum()
        }
        AgentMessage::Llm(Message::ToolResult(tr)) => {
            tr.content.iter().map(chars_for_user_block).sum()
        }
        AgentMessage::Custom { data, .. } => {
            serde_json::to_string(data).map(|s| s.len()).unwrap_or(0)
        }
    }
}

fn chars_for_user_block(block: &UserBlock) -> usize {
    match block {
        UserBlock::Text { text } => text.len(),
        UserBlock::Image { .. } => IMAGE_TOKEN_ESTIMATE * CHARS_PER_TOKEN,
    }
}

fn chars_for_content_block(block: &ContentBlock) -> usize {
    match block {
        ContentBlock::Text { text, .. } => text.len(),
        ContentBlock::Thinking { thinking, .. } => thinking.len(),
        ContentBlock::Image { .. } => IMAGE_TOKEN_ESTIMATE * CHARS_PER_TOKEN,
        ContentBlock::ToolCall { name, arguments, .. } => {
            name.len() + serde_json::to_string(arguments).map(|s| s.len()).unwrap_or(0)
        }
    }
}

// ---------------------------------------------------------------------------
// Budget calculation
// ---------------------------------------------------------------------------

/// Compute the token budget for a model.
///
/// Returns: `(context_window * 0.75) - max_tokens - SYSTEM_PROMPT_ESTIMATE`, clamped to 0.
pub fn compute_budget(model: &Model) -> usize {
    let gross = (model.context_window as f64 * BUDGET_FACTOR) as usize;
    let deductions = (model.max_tokens as usize).saturating_add(SYSTEM_PROMPT_ESTIMATE);
    gross.saturating_sub(deductions)
}

// ---------------------------------------------------------------------------
// Turn detection
// ---------------------------------------------------------------------------

/// A contiguous slice of messages forming one interaction cycle.
#[derive(Debug, Clone)]
struct Turn {
    start: usize,  // inclusive index into the messages slice
    end: usize,    // exclusive index
    tokens: usize, // estimated tokens for messages[start..end]
}

/// Group messages into turns.
///
/// A new turn starts at each User message.  Messages before the first User
/// message (e.g. a bare assistant opening) are folded into turn 0.
fn detect_turns(messages: &[AgentMessage]) -> Vec<Turn> {
    if messages.is_empty() {
        return vec![];
    }

    let mut turns: Vec<Turn> = Vec::new();
    let mut turn_start = 0usize;

    for i in 1..messages.len() {
        if matches!(&messages[i], AgentMessage::Llm(Message::User(_))) {
            turns.push(Turn {
                start: turn_start,
                end: i,
                tokens: estimate_tokens(&messages[turn_start..i]),
            });
            turn_start = i;
        }
    }
    // Final (or only) turn.
    turns.push(Turn {
        start: turn_start,
        end: messages.len(),
        tokens: estimate_tokens(&messages[turn_start..]),
    });

    turns
}

// ---------------------------------------------------------------------------
// Tier 1: tool output truncation
// ---------------------------------------------------------------------------

/// Truncate a text block that exceeds the per-tool-output limits.
/// Keeps the first 40% and last 40% of lines, inserting a marker in the middle.
fn maybe_truncate_text(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let total_lines = lines.len();
    let total_chars = text.len();

    if total_chars <= MAX_TOOL_OUTPUT_CHARS && total_lines <= MAX_TOOL_OUTPUT_LINES {
        return text.to_string();
    }

    let head_count = (total_lines as f64 * 0.4) as usize;
    let tail_count = (total_lines as f64 * 0.4) as usize;
    let omitted = total_lines.saturating_sub(head_count + tail_count);

    let head = lines[..head_count].join("\n");
    let tail_start = lines.len().saturating_sub(tail_count);
    let tail = lines[tail_start..].join("\n");

    format!("{}\n\n[... truncated {} lines ...]\n\n{}", head, omitted, tail)
}

/// Apply tier-1 truncation in-place across all ToolResult messages.
fn truncate_tool_outputs(messages: &mut [AgentMessage]) {
    for msg in messages.iter_mut() {
        if let AgentMessage::Llm(Message::ToolResult(tr)) = msg {
            for block in tr.content.iter_mut() {
                if let UserBlock::Text { text } = block {
                    *text = maybe_truncate_text(text);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tier 2: observation masking
// ---------------------------------------------------------------------------

/// Replace old-turn messages with compact placeholders.
///
/// - The very first User message is always preserved (original task).
/// - Subsequent User messages in old turns are dropped (save tokens).
/// - ToolResult content replaced with "[output from <name> omitted]".
/// - Assistant text/thinking cleared; ToolCall blocks kept (show intent).
/// - Custom messages kept verbatim (invisible to the LLM).
fn mask_old_turns(messages: Vec<AgentMessage>, turns: &[Turn], keep_from: usize) -> Vec<AgentMessage> {
    let mut result = Vec::with_capacity(messages.len());
    let mut first_user_kept = false;

    for (i, msg) in messages.into_iter().enumerate() {
        // Determine which turn this message belongs to.
        let turn_idx = turns
            .iter()
            .position(|t| i >= t.start && i < t.end)
            .unwrap_or(0);

        if turn_idx >= keep_from {
            // Recent turn — keep verbatim.
            result.push(msg);
            continue;
        }

        // Old turn — apply masking.
        match msg {
            AgentMessage::Llm(Message::User(_)) => {
                if !first_user_kept {
                    // Always keep the original task (first user message).
                    first_user_kept = true;
                    result.push(msg);
                }
                // All other old-turn user messages are dropped to save tokens.
            }
            AgentMessage::Llm(Message::Assistant(mut am)) => {
                // Clear text and thinking; keep ToolCall blocks so the LLM
                // can see what was requested without the verbose response.
                for block in am.content.iter_mut() {
                    match block {
                        ContentBlock::Text { text, .. } => *text = String::new(),
                        ContentBlock::Thinking { thinking, .. } => *thinking = String::new(),
                        ContentBlock::ToolCall { .. } | ContentBlock::Image { .. } => {}
                    }
                }
                result.push(AgentMessage::Llm(Message::Assistant(am)));
            }
            AgentMessage::Llm(Message::ToolResult(mut tr)) => {
                let name = tr.tool_name.clone();
                tr.content = vec![UserBlock::Text {
                    text: format!("[output from {} omitted]", name),
                }];
                result.push(AgentMessage::Llm(Message::ToolResult(tr)));
            }
            AgentMessage::Custom { .. } => {
                // Custom messages are invisible to the LLM; preserve them.
                result.push(msg);
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Overflow fallback: aggressive per-tool-result truncation
// ---------------------------------------------------------------------------

/// Truncate to 20% head + 20% tail (line-based).
fn truncate_text_aggressive(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len();
    let head_count = (total as f64 * 0.2) as usize;
    let tail_count = (total as f64 * 0.2) as usize;
    let omitted = total.saturating_sub(head_count + tail_count);

    let head = lines[..head_count].join("\n");
    let tail_start = lines.len().saturating_sub(tail_count);
    let tail = lines[tail_start..].join("\n");

    format!("{}\n\n[... truncated {} lines ...]\n\n{}", head, omitted, tail)
}

/// Iteratively find and aggressively truncate the largest remaining tool result
/// until under budget or no meaningful content remains.
fn overflow_fallback(messages: &mut [AgentMessage], budget: usize) -> usize {
    const MIN_MEANINGFUL_CHARS: usize = 100;

    loop {
        let current = estimate_tokens(messages);
        if current <= budget {
            return current;
        }

        // Find the largest tool-result text block.
        let mut best: Option<(usize, usize)> = None; // (msg_idx, total_chars)
        for (i, msg) in messages.iter().enumerate() {
            if let AgentMessage::Llm(Message::ToolResult(tr)) = msg {
                let size: usize = tr
                    .content
                    .iter()
                    .filter_map(|b| {
                        if let UserBlock::Text { text } = b {
                            Some(text.len())
                        } else {
                            None
                        }
                    })
                    .sum();
                if size > best.map_or(0, |(_, s)| s) {
                    best = Some((i, size));
                }
            }
        }

        match best {
            // Nothing substantial left to truncate.
            None | Some((_, 0..=MIN_MEANINGFUL_CHARS)) => {
                return estimate_tokens(messages);
            }
            Some((idx, _)) => {
                if let AgentMessage::Llm(Message::ToolResult(ref mut tr)) = messages[idx] {
                    for block in tr.content.iter_mut() {
                        if let UserBlock::Text { text } = block {
                            *text = truncate_text_aggressive(text);
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Compact `messages` to fit within the model's context window budget.
///
/// Returns the messages unchanged if already under budget (zero overhead).
/// Logs a summary to stderr when compaction fires.
pub fn compact_messages(messages: Vec<AgentMessage>, model: &Model) -> Vec<AgentMessage> {
    let budget = compute_budget(model);
    let before_tokens = estimate_tokens(&messages);

    if before_tokens <= budget {
        return messages;
    }

    // --- Tier 1: truncate oversized tool outputs ---
    let mut messages = messages;
    truncate_tool_outputs(&mut messages);
    let after_t1 = estimate_tokens(&messages);

    if after_t1 <= budget {
        eprintln!("[compact] {} -> {} tokens (tool output truncation)", before_tokens, after_t1);
        return messages;
    }

    // --- Tier 2: mask old turns ---
    let turns = detect_turns(&messages);

    // Walk backwards from the most recent turn, accumulating token estimates,
    // until we've filled the budget or exhausted all turns.
    let mut kept_tokens: usize = 0;
    let mut keep_from = turns.len(); // initialised high; will be driven down

    for i in (0..turns.len()).rev() {
        let turn_tokens = turns[i].tokens;
        // Always include the most recent turn; include older ones while they fit.
        let always_include = i == turns.len().saturating_sub(1);
        if always_include || kept_tokens.saturating_add(turn_tokens) <= budget {
            kept_tokens = kept_tokens.saturating_add(turn_tokens);
            keep_from = i;
        } else {
            break;
        }
    }

    // Clamp so keep_from is a valid turn index.
    if !turns.is_empty() {
        keep_from = keep_from.min(turns.len() - 1);
    }

    let turns_masked = keep_from;
    let messages = mask_old_turns(messages, &turns, keep_from);
    let after_t2 = estimate_tokens(&messages);

    if after_t2 <= budget {
        eprintln!("[compact] {} -> {} tokens ({} turns masked)", before_tokens, after_t2, turns_masked);
        return messages;
    }

    // --- Overflow fallback: aggressive per-tool-result truncation ---
    let mut messages = messages;
    let after_fallback = overflow_fallback(&mut messages, budget);
    eprintln!(
        "[compact] {} -> {} tokens ({} turns masked, overflow fallback applied)",
        before_tokens, after_fallback, turns_masked
    );

    messages
}
