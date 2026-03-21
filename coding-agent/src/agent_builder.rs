//! Shared agent construction logic used by both the interactive CLI and the serve mode.

use std::sync::Arc;

use agent::types::AgentTool;
use agent::{Agent, AgentOptions, AgentStateInit};
use ai::types::Model;
use anyhow::{anyhow, Result};

use crate::config::{load_config, TauConfig};
use crate::tools;

/// Configuration for building an agent, independent of CLI or serve mode.
pub struct AgentBuildConfig {
    pub model_id: Option<String>,
    pub system_prompt: Option<String>,
    pub tools: Option<Vec<String>>,
    pub max_turns: Option<u32>,
}

/// Result of building an agent — the agent plus resolved metadata.
pub struct BuiltAgent {
    pub agent: Agent,
    pub config: TauConfig,
    pub model_id: String,
    pub model_provider: String,
    pub system_prompt_text: String,
}

/// Build an Agent with all provider/key/model resolution handled.
///
/// This extracts the common setup from main.rs so both the interactive CLI
/// and the serve mode can share it.
pub async fn build_agent(build_config: AgentBuildConfig) -> Result<BuiltAgent> {
    let config = load_config();

    // Resolve model: explicit > TAU_MODEL env > config > default
    let model_id = build_config
        .model_id
        .or_else(|| std::env::var("TAU_MODEL").ok())
        .unwrap_or_else(|| config.model.clone());

    let max_turns: Option<u32> = std::env::var("TAU_MAX_TURNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .or(config.max_turns)
        .or(build_config.max_turns);

    // Register providers and resolve model
    ai::register_builtin_providers();

    let model = ai::models::find_model(&model_id)
        .ok_or_else(|| anyhow!("Model '{}' not found in registry", model_id))?;
    let mut model: Model = (*model).clone();

    // Resolve API key / auth based on model provider.
    let codex_auth: Option<Arc<ai::codex_auth::CodexAuth>> =
        if model.provider == "anthropic" || std::env::var("OPENAI_API_KEY").is_ok() {
            None
        } else {
            match ai::codex_auth::CodexAuth::load() {
                Ok(auth) => {
                    eprintln!("[auth] Using Codex OAuth (~/.codex/auth.json)");
                    Some(Arc::new(auth))
                }
                Err(_) => None,
            }
        };

    // When using Codex OAuth, redirect requests to the ChatGPT backend.
    if let Some(ref auth) = codex_auth {
        model.base_url = ai::codex_auth::CHATGPT_BACKEND_URL.to_string();
        if let Some(id) = auth.account_id().await {
            let headers = model
                .headers
                .get_or_insert_with(std::collections::HashMap::new);
            headers.insert("ChatGPT-Account-ID".to_string(), id);
        }
    }

    // Validate auth
    let explicit_api_key: Option<String> = match model.provider.as_str() {
        "anthropic" => Some(std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
            anyhow!(
                "ANTHROPIC_API_KEY not set (required for model '{}')",
                model_id
            )
        })?),
        _ => {
            let env_key = std::env::var("OPENAI_API_KEY").ok();
            if env_key.is_none() && codex_auth.is_none() {
                return Err(anyhow!(
                    "No API key for model '{}'. Set OPENAI_API_KEY or run `codex login`.",
                    model_id
                ));
            }
            env_key
        }
    };

    // Build tools
    let tool_list: Vec<Arc<dyn AgentTool>> = if let Some(ref tool_names) = build_config.tools {
        eprintln!("[tools] enabled: {}", tool_names.join(", "));
        tools::tools_from_allowlist(tool_names, &config.edit_mode)
    } else if let Some(ref tool_names) = config.tools {
        eprintln!("[tools] enabled: {}", tool_names.join(", "));
        tools::tools_from_allowlist(tool_names, &config.edit_mode)
    } else {
        tools::tools_for_edit_mode(&config.edit_mode)
    };

    // Build system prompt
    let system_prompt_text = build_config.system_prompt.unwrap_or_else(|| {
        crate::system_prompt::build_system_prompt(
            &tool_list,
            &std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .to_string_lossy(),
        )
    });

    let model_provider = model.provider.clone();
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentStateInit {
            model: Some(model),
            system_prompt: Some(system_prompt_text.clone()),
            tools: Some(tool_list),
            thinking_level: None,
        }),
        convert_to_llm: None,
        transform_context: None,
        stream_fn: None,
        steering_mode: None,
        follow_up_mode: None,
        session_id: None,
        get_api_key: Some({
            let codex = codex_auth.clone();
            let key = explicit_api_key.clone();
            Arc::new(move |_provider: String| {
                let codex = codex.clone();
                let key = key.clone();
                Box::pin(async move {
                    if let Some(k) = key {
                        return Some(k);
                    }
                    if let Some(ref auth) = codex {
                        match auth.access_token().await {
                            Ok(token) => return Some(token),
                            Err(e) => {
                                eprintln!("Warning: Codex OAuth error: {}", e);
                            }
                        }
                    }
                    None
                })
            })
        }),
        thinking_budgets: None,
        transport: None,
        max_retry_delay_ms: None,
        max_turns,
    });

    Ok(BuiltAgent {
        agent,
        config,
        model_id,
        model_provider,
        system_prompt_text,
    })
}
