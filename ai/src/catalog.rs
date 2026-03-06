//! Built-in model catalog — ported from models.generated.ts
//!
//! Providers included: anthropic, openai, kimi-coding.

use crate::types::{known_api, Model, ModelCost};

fn m(
    id: &str,
    name: &str,
    api: &str,
    provider: &str,
    base_url: &str,
    reasoning: bool,
    input: &[&str],
    cost_in: f64,
    cost_out: f64,
    cost_cr: f64,
    cost_cw: f64,
    ctx: u64,
    max_tok: u64,
) -> Model {
    Model {
        id: id.into(),
        name: name.into(),
        api: api.into(),
        provider: provider.into(),
        base_url: base_url.into(),
        reasoning,
        input: input.iter().map(|s| s.to_string()).collect(),
        cost: ModelCost { input: cost_in, output: cost_out, cache_read: cost_cr, cache_write: cost_cw },
        context_window: ctx,
        max_tokens: max_tok,
        headers: None,
        compat: None,
    }
}

const ANTHROPIC_URL: &str = "https://api.anthropic.com";
const OPENAI_URL: &str = "https://api.openai.com/v1";
const KIMI_URL: &str = "https://api.kimi.com/coding";
const AM: &str = known_api::ANTHROPIC_MESSAGES;
const OR: &str = known_api::OPENAI_RESPONSES;
const TI: &[&str] = &["text", "image"];
const T: &[&str] = &["text"];

pub fn builtin_models() -> Vec<Model> {
    vec![
        // -----------------------------------------------------------------------
        // anthropic
        // -----------------------------------------------------------------------
        m("claude-3-5-haiku-20241022",       "Claude Haiku 3.5",               AM, "anthropic", ANTHROPIC_URL, false, TI,  0.8,  4.0,  0.08,  1.0,   200_000,   8_192),
        m("claude-3-5-haiku-latest",         "Claude Haiku 3.5 (latest)",       AM, "anthropic", ANTHROPIC_URL, false, TI,  0.8,  4.0,  0.08,  1.0,   200_000,   8_192),
        m("claude-3-5-sonnet-20240620",      "Claude Sonnet 3.5",               AM, "anthropic", ANTHROPIC_URL, false, TI,  3.0, 15.0,  0.3,   3.75,  200_000,   8_192),
        m("claude-3-5-sonnet-20241022",      "Claude Sonnet 3.5 v2",            AM, "anthropic", ANTHROPIC_URL, false, TI,  3.0, 15.0,  0.3,   3.75,  200_000,   8_192),
        m("claude-3-7-sonnet-20250219",      "Claude Sonnet 3.7",               AM, "anthropic", ANTHROPIC_URL, true,  TI,  3.0, 15.0,  0.3,   3.75,  200_000,  64_000),
        m("claude-3-7-sonnet-latest",        "Claude Sonnet 3.7 (latest)",      AM, "anthropic", ANTHROPIC_URL, true,  TI,  3.0, 15.0,  0.3,   3.75,  200_000,  64_000),
        m("claude-3-haiku-20240307",         "Claude Haiku 3",                  AM, "anthropic", ANTHROPIC_URL, false, TI,  0.25, 1.25, 0.03,  0.3,   200_000,   4_096),
        m("claude-3-opus-20240229",          "Claude Opus 3",                   AM, "anthropic", ANTHROPIC_URL, false, TI, 15.0, 75.0,  1.5,  18.75, 200_000,   4_096),
        m("claude-3-sonnet-20240229",        "Claude Sonnet 3",                 AM, "anthropic", ANTHROPIC_URL, false, TI,  3.0, 15.0,  0.3,   0.3,   200_000,   4_096),
        m("claude-haiku-4-5",               "Claude Haiku 4.5 (latest)",        AM, "anthropic", ANTHROPIC_URL, true,  TI,  1.0,  5.0,  0.1,   1.25,  200_000,  64_000),
        m("claude-haiku-4-5-20251001",      "Claude Haiku 4.5",                 AM, "anthropic", ANTHROPIC_URL, true,  TI,  1.0,  5.0,  0.1,   1.25,  200_000,  64_000),
        m("claude-opus-4-0",                "Claude Opus 4 (latest)",           AM, "anthropic", ANTHROPIC_URL, true,  TI, 15.0, 75.0,  1.5,  18.75, 200_000,  32_000),
        m("claude-opus-4-1",                "Claude Opus 4.1 (latest)",         AM, "anthropic", ANTHROPIC_URL, true,  TI, 15.0, 75.0,  1.5,  18.75, 200_000,  32_000),
        m("claude-opus-4-1-20250805",       "Claude Opus 4.1",                  AM, "anthropic", ANTHROPIC_URL, true,  TI, 15.0, 75.0,  1.5,  18.75, 200_000,  32_000),
        m("claude-opus-4-20250514",         "Claude Opus 4",                    AM, "anthropic", ANTHROPIC_URL, true,  TI, 15.0, 75.0,  1.5,  18.75, 200_000,  32_000),
        m("claude-opus-4-5",                "Claude Opus 4.5 (latest)",         AM, "anthropic", ANTHROPIC_URL, true,  TI,  5.0, 25.0,  0.5,   6.25,  200_000,  64_000),
        m("claude-opus-4-5-20251101",       "Claude Opus 4.5",                  AM, "anthropic", ANTHROPIC_URL, true,  TI,  5.0, 25.0,  0.5,   6.25,  200_000,  64_000),
        m("claude-opus-4-6",                "Claude Opus 4.6",                  AM, "anthropic", ANTHROPIC_URL, true,  TI,  5.0, 25.0,  0.5,   6.25,  200_000, 128_000),
        m("claude-sonnet-4-0",              "Claude Sonnet 4 (latest)",         AM, "anthropic", ANTHROPIC_URL, true,  TI,  3.0, 15.0,  0.3,   3.75,  200_000,  64_000),
        m("claude-sonnet-4-20250514",       "Claude Sonnet 4",                  AM, "anthropic", ANTHROPIC_URL, true,  TI,  3.0, 15.0,  0.3,   3.75,  200_000,  64_000),
        m("claude-sonnet-4-5",              "Claude Sonnet 4.5 (latest)",       AM, "anthropic", ANTHROPIC_URL, true,  TI,  3.0, 15.0,  0.3,   3.75,  200_000,  64_000),
        m("claude-sonnet-4-5-20250929",     "Claude Sonnet 4.5",                AM, "anthropic", ANTHROPIC_URL, true,  TI,  3.0, 15.0,  0.3,   3.75,  200_000,  64_000),
        m("claude-sonnet-4-6",              "Claude Sonnet 4.6",                AM, "anthropic", ANTHROPIC_URL, true,  TI,  3.0, 15.0,  0.3,   3.75,  200_000,  64_000),

        // -----------------------------------------------------------------------
        // openai
        // -----------------------------------------------------------------------
        m("codex-mini-latest",   "Codex Mini",          OR, "openai", OPENAI_URL, true,  T,    1.5,   6.0,   0.375, 0.0,  200_000, 100_000),
        m("gpt-4",               "GPT-4",               OR, "openai", OPENAI_URL, false, T,   30.0,  60.0,   0.0,   0.0,    8_192,   8_192),
        m("gpt-4-turbo",         "GPT-4 Turbo",         OR, "openai", OPENAI_URL, false, TI,  10.0,  30.0,   0.0,   0.0,  128_000,   4_096),
        m("gpt-4.1",             "GPT-4.1",             OR, "openai", OPENAI_URL, false, TI,   2.0,   8.0,   0.5,   0.0, 1_047_576, 32_768),
        m("gpt-4.1-mini",        "GPT-4.1 mini",        OR, "openai", OPENAI_URL, false, TI,   0.4,   1.6,   0.1,   0.0, 1_047_576, 32_768),
        m("gpt-4.1-nano",        "GPT-4.1 nano",        OR, "openai", OPENAI_URL, false, TI,   0.1,   0.4,   0.03,  0.0, 1_047_576, 32_768),
        m("gpt-4o",              "GPT-4o",              OR, "openai", OPENAI_URL, false, TI,   2.5,  10.0,   1.25,  0.0,  128_000,  16_384),
        m("gpt-4o-2024-05-13",   "GPT-4o (2024-05-13)", OR, "openai", OPENAI_URL, false, TI,   5.0,  15.0,   0.0,   0.0,  128_000,   4_096),
        m("gpt-4o-2024-08-06",   "GPT-4o (2024-08-06)", OR, "openai", OPENAI_URL, false, TI,   2.5,  10.0,   1.25,  0.0,  128_000,  16_384),
        m("gpt-4o-2024-11-20",   "GPT-4o (2024-11-20)", OR, "openai", OPENAI_URL, false, TI,   2.5,  10.0,   1.25,  0.0,  128_000,  16_384),
        m("gpt-4o-mini",         "GPT-4o mini",         OR, "openai", OPENAI_URL, false, TI,   0.15,  0.6,   0.08,  0.0,  128_000,  16_384),
        m("gpt-5",               "GPT-5",               OR, "openai", OPENAI_URL, true,  TI,   1.25, 10.0,   0.125, 0.0,  400_000, 128_000),
        m("gpt-5-chat-latest",   "GPT-5 Chat Latest",   OR, "openai", OPENAI_URL, false, TI,   1.25, 10.0,   0.125, 0.0,  128_000,  16_384),
        m("gpt-5-codex",         "GPT-5-Codex",         OR, "openai", OPENAI_URL, true,  TI,   1.25, 10.0,   0.125, 0.0,  400_000, 128_000),
        m("gpt-5-mini",          "GPT-5 Mini",          OR, "openai", OPENAI_URL, true,  TI,   0.25,  2.0,   0.025, 0.0,  400_000, 128_000),
        m("gpt-5-nano",          "GPT-5 Nano",          OR, "openai", OPENAI_URL, true,  TI,   0.05,  0.4,   0.005, 0.0,  400_000, 128_000),
        m("gpt-5-pro",           "GPT-5 Pro",           OR, "openai", OPENAI_URL, true,  TI,  15.0, 120.0,   0.0,   0.0,  400_000, 272_000),
        m("gpt-5.1",             "GPT-5.1",             OR, "openai", OPENAI_URL, true,  TI,   1.25, 10.0,   0.13,  0.0,  400_000, 128_000),
        m("gpt-5.1-chat-latest", "GPT-5.1 Chat",        OR, "openai", OPENAI_URL, true,  TI,   1.25, 10.0,   0.125, 0.0,  128_000,  16_384),
        m("gpt-5.1-codex",       "GPT-5.1 Codex",       OR, "openai", OPENAI_URL, true,  TI,   1.25, 10.0,   0.125, 0.0,  400_000, 128_000),
        m("gpt-5.1-codex-max",   "GPT-5.1 Codex Max",   OR, "openai", OPENAI_URL, true,  TI,   1.25, 10.0,   0.125, 0.0,  400_000, 128_000),
        m("gpt-5.1-codex-mini",  "GPT-5.1 Codex mini",  OR, "openai", OPENAI_URL, true,  TI,   0.25,  2.0,   0.025, 0.0,  400_000, 128_000),
        m("gpt-5.2",             "GPT-5.2",             OR, "openai", OPENAI_URL, true,  TI,   1.75, 14.0,   0.175, 0.0,  400_000, 128_000),
        m("gpt-5.2-chat-latest", "GPT-5.2 Chat",        OR, "openai", OPENAI_URL, true,  TI,   1.75, 14.0,   0.175, 0.0,  128_000,  16_384),
        m("gpt-5.2-codex",       "GPT-5.2 Codex",       OR, "openai", OPENAI_URL, true,  TI,   1.75, 14.0,   0.175, 0.0,  400_000, 128_000),
        m("gpt-5.2-pro",         "GPT-5.2 Pro",         OR, "openai", OPENAI_URL, true,  TI,  21.0, 168.0,   0.0,   0.0,  400_000, 128_000),
        m("gpt-5.3-codex",       "GPT-5.3 Codex",       OR, "openai", OPENAI_URL, true,  TI,   1.75, 14.0,   0.175, 0.0,  400_000, 128_000),
        m("gpt-5.3-codex-spark", "GPT-5.3 Codex Spark", OR, "openai", OPENAI_URL, true,  TI,   1.75, 14.0,   0.175, 0.0,  128_000,  32_000),
        m("o1",                  "o1",                  OR, "openai", OPENAI_URL, true,  TI,  15.0,  60.0,   7.5,   0.0,  200_000, 100_000),
        m("o1-pro",              "o1-pro",              OR, "openai", OPENAI_URL, true,  TI, 150.0, 600.0,   0.0,   0.0,  200_000, 100_000),
        m("o3",                  "o3",                  OR, "openai", OPENAI_URL, true,  TI,   2.0,   8.0,   0.5,   0.0,  200_000, 100_000),
        m("o3-deep-research",    "o3-deep-research",    OR, "openai", OPENAI_URL, true,  TI,  10.0,  40.0,   2.5,   0.0,  200_000, 100_000),
        m("o3-mini",             "o3-mini",             OR, "openai", OPENAI_URL, true,  T,    1.1,   4.4,   0.55,  0.0,  200_000, 100_000),
        m("o3-pro",              "o3-pro",              OR, "openai", OPENAI_URL, true,  TI,  20.0,  80.0,   0.0,   0.0,  200_000, 100_000),
        m("o4-mini",             "o4-mini",             OR, "openai", OPENAI_URL, true,  TI,   1.1,   4.4,   0.28,  0.0,  200_000, 100_000),
        m("o4-mini-deep-research","o4-mini-deep-research",OR,"openai",OPENAI_URL, true,  TI,   2.0,   8.0,   0.5,   0.0,  200_000, 100_000),

        // -----------------------------------------------------------------------
        // kimi-coding
        // -----------------------------------------------------------------------
        m("k2p5",           "Kimi K2.5",       AM, "kimi-coding", KIMI_URL, true,  TI, 0.0, 0.0, 0.0, 0.0, 262_144, 32_768),
        m("kimi-k2-thinking","Kimi K2 Thinking",AM, "kimi-coding", KIMI_URL, true,  T,  0.0, 0.0, 0.0, 0.0, 262_144, 32_768),
    ]
}
