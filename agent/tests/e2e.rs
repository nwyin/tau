//! Mirrors: packages/agent/test/e2e.test.ts
//! End-to-end integration tests against live LLM providers.

mod common;
use common::*;

use agent::agent::{Agent, AgentOptions, AgentStateInit};
use agent::types::ThinkingLevel;
use ai::models::get_model;

async fn basic_prompt(model: ai::types::Model) {
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentStateInit {
            system_prompt: Some("You are a helpful assistant. Keep your responses concise.".into()),
            model: Some(model),
            thinking_level: Some(ThinkingLevel::Off),
            tools: Some(vec![]),
        }),
        ..default_agent_opts_no_model()
    });

    agent.prompt("What is 2+2? Answer with just the number.").await.unwrap();

    agent.with_state(|s| {
        assert!(!s.is_streaming);
        assert_eq!(s.messages.len(), 2);
        assert_eq!(s.messages[0].role(), "user");
        assert_eq!(s.messages[1].role(), "assistant");
    });
}

async fn tool_execution(model: ai::types::Model) {
    // TODO: wire up a calculate tool once AgentTool infrastructure is testable
    todo!("tool execution e2e")
}

// ---------------------------------------------------------------------------
// Per-provider e2e tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn anthropic_basic_prompt() {
    let model = get_model("anthropic", "claude-3-5-haiku-20241022").unwrap();
    basic_prompt((*model).clone()).await;
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn openai_basic_prompt() {
    let model = get_model("openai", "gpt-4o-mini").unwrap();
    basic_prompt((*model).clone()).await;
}

#[tokio::test]
#[ignore = "requires GEMINI_API_KEY"]
async fn google_basic_prompt() {
    let model = get_model("google", "gemini-2.5-flash").unwrap();
    basic_prompt((*model).clone()).await;
}

#[tokio::test]
#[ignore = "requires AWS credentials"]
async fn bedrock_basic_prompt() {
    let model = get_model("amazon-bedrock", "global.anthropic.claude-sonnet-4-5-20250929-v1:0").unwrap();
    basic_prompt((*model).clone()).await;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_agent_opts_no_model() -> AgentOptions {
    AgentOptions {
        initial_state: None,
        convert_to_llm: None,
        transform_context: None,
        steering_mode: None,
        follow_up_mode: None,
        session_id: None,
        get_api_key: None,
        thinking_budgets: None,
        transport: None,
        max_retry_delay_ms: None,
    }
}
