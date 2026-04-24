//! Shared agent construction logic used by both the interactive CLI and the serve mode.

use std::sync::Arc;

use agent::types::AgentTool;
use agent::{Agent, AgentOptions, AgentStateInit};
use ai::types::Model;
use anyhow::{anyhow, Result};

use agent::orchestrator::OrchestratorState;

use crate::config::{load_config, TauConfig};
use crate::permissions::{self, PermissionService};
use crate::skills::{self, Skill};
use crate::tools;

/// Configuration for building an agent, independent of CLI or serve mode.
pub struct AgentBuildConfig {
    pub model_id: Option<String>,
    pub system_prompt: Option<String>,
    pub tools: Option<Vec<String>>,
    pub max_turns: Option<u32>,
    pub yolo: bool,
    pub thinking: Option<String>,
    pub permission_prompt_fn: Option<permissions::PromptFn>,
    pub no_skills: bool,
    pub skill_paths: Vec<String>,
}

/// Result of building an agent — the agent plus resolved metadata.
pub struct BuiltAgent {
    pub agent: Agent,
    pub config: TauConfig,
    pub model_id: String,
    pub model_provider: String,
    pub system_prompt_text: String,
    pub tool_names: Vec<String>,
    pub skills: Vec<Skill>,
    pub permission_service: Arc<PermissionService>,
    pub orchestrator: Arc<OrchestratorState>,
    /// Startup messages (warnings, info) to display to the user.
    pub startup_messages: Vec<String>,
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

    // Initialize API concurrency limiter: env > config > default(10)
    let max_api = std::env::var("TAU_MAX_API_REQUESTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .or(config.max_api_requests.map(|v| v as usize))
        .unwrap_or(10usize);
    ai::concurrency::init(max_api);

    let model = ai::models::find_model(&model_id)
        .ok_or_else(|| anyhow!("Model '{}' not found in registry", model_id))?;
    let mut model: Model = (*model).clone();

    // Collect startup messages instead of printing directly
    let mut startup_messages = Vec::new();

    // Resolve API key / auth based on model provider.
    let codex_auth: Option<Arc<ai::codex_auth::CodexAuth>> = if model.provider == "anthropic"
        || std::env::var("OPENAI_API_KEY").is_ok()
    {
        None
    } else {
        match ai::codex_auth::CodexAuth::load() {
            Ok(auth) => {
                startup_messages.push("[auth] Using Codex OAuth (~/.codex/auth.json)".to_string());
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
        "openrouter" => Some(std::env::var("OPENROUTER_API_KEY").map_err(|_| {
            anyhow!(
                "OPENROUTER_API_KEY not set (required for model '{}')",
                model_id
            )
        })?),
        "groq" => Some(
            std::env::var("GROQ_API_KEY")
                .map_err(|_| anyhow!("GROQ_API_KEY not set (required for model '{}')", model_id))?,
        ),
        "together" => Some(std::env::var("TOGETHER_API_KEY").map_err(|_| {
            anyhow!(
                "TOGETHER_API_KEY not set (required for model '{}')",
                model_id
            )
        })?),
        "deepseek" => Some(std::env::var("DEEPSEEK_API_KEY").map_err(|_| {
            anyhow!(
                "DEEPSEEK_API_KEY not set (required for model '{}')",
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

    // Build get_api_key closure (shared between agent and orchestration tools)
    let get_api_key: agent::types::GetApiKeyFn = {
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
    };

    // Create orchestrator state
    let max_threads = std::env::var("TAU_MAX_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10usize);
    let orchestrator = OrchestratorState::with_max_threads(max_threads);

    // Build tools
    let direct_tools: Vec<Arc<dyn AgentTool>> = if let Some(ref tool_names) = build_config.tools {
        startup_messages.push(format!("[tools] enabled: {}", tool_names.join(", ")));
        tools::tools_from_allowlist(tool_names)
    } else if let Some(ref tool_names) = config.tools {
        startup_messages.push(format!("[tools] enabled: {}", tool_names.join(", ")));
        tools::tools_from_allowlist(tool_names)
    } else {
        tools::default_tools()
    };

    // Build orchestration tools that need runtime state. py_repl is added after
    // permission wrapping so its reverse-RPC dispatch uses the same surface.
    let orch = tools::orchestration_core_tools(
        orchestrator.clone(),
        Some(get_api_key.clone()),
        model.clone(),
        config.models.clone(),
    );

    // Warn about missing optional API keys for included tools
    let has_web_search = direct_tools.iter().any(|t| t.name() == "web_search");
    if has_web_search
        && std::env::var("EXA_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .is_none()
    {
        startup_messages.push(
            "[warn] web_search tool enabled but EXA_API_KEY not set — get one at https://exa.ai"
                .to_string(),
        );
    }

    // Build permission service and wrap tools
    let config_perms = config.permissions.clone().unwrap_or_default();
    let perm_svc = PermissionService::new(&config_perms, build_config.yolo);
    if let Some(prompt_fn) = build_config.permission_prompt_fn {
        perm_svc.set_prompt_fn(prompt_fn);
    }
    let permission_service = Arc::new(perm_svc);
    let wrapped_direct_tools =
        permissions::wrap_tools(direct_tools, Arc::clone(&permission_service));
    let wrapped_orch_tools = permissions::wrap_tools(orch.tools, Arc::clone(&permission_service));

    let generic_tools = wrapped_direct_tools
        .iter()
        .map(|tool| (tool.name().to_string(), Arc::clone(tool)))
        .collect();
    let wrapped_thread_tool = wrapped_orch_tools
        .iter()
        .find(|tool| tool.name() == "thread")
        .cloned()
        .unwrap_or_else(|| orch.thread_tool.clone());
    let wrapped_query_tool = wrapped_orch_tools
        .iter()
        .find(|tool| tool.name() == "query")
        .cloned()
        .unwrap_or_else(|| orch.query_tool.clone());
    let wrapped_document_tool = wrapped_orch_tools
        .iter()
        .find(|tool| tool.name() == "document")
        .cloned()
        .unwrap_or_else(|| orch.document_tool.clone());
    let py_repl_tool = tools::py_repl::PyReplTool::arc_with_tools(
        wrapped_thread_tool,
        wrapped_query_tool,
        wrapped_document_tool,
        generic_tools,
    );
    let mut wrapped_py_repl =
        permissions::wrap_tools(vec![py_repl_tool], Arc::clone(&permission_service));

    let mut tool_list = wrapped_direct_tools;
    tool_list.extend(wrapped_orch_tools);
    tool_list.append(&mut wrapped_py_repl);

    // Load skills
    let no_skills = build_config.no_skills || config.skills.map(|s| !s).unwrap_or(false);
    let extra_paths: Vec<std::path::PathBuf> = build_config
        .skill_paths
        .iter()
        .map(std::path::PathBuf::from)
        .collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let loaded_skills = skills::load_skills(&cwd, no_skills, &extra_paths);
    for diag in &loaded_skills.diagnostics {
        startup_messages.push(format!(
            "Warning: skill {}: {}",
            diag.path.display(),
            diag.message
        ));
    }
    if !loaded_skills.skills.is_empty() {
        startup_messages.push(format!(
            "[skills] loaded: {}",
            loaded_skills
                .skills
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    // Build system prompt
    let system_prompt_text = build_config.system_prompt.unwrap_or_else(|| {
        crate::system_prompt::build_system_prompt(
            &tool_list,
            &loaded_skills.skills,
            &cwd.to_string_lossy(),
        )
    });

    // Resolve thinking level: CLI > config > default (off)
    let thinking_level_str = build_config
        .thinking
        .or_else(|| config.thinking.clone())
        .unwrap_or_else(|| "off".to_string());
    let thinking_level: agent::types::ThinkingLevel =
        serde_json::from_value(serde_json::Value::String(thinking_level_str)).unwrap_or_default();

    let tool_names = tool_list
        .iter()
        .map(|tool| tool.name().to_string())
        .collect();
    let model_provider = model.provider.clone();
    let model_for_compact = model.clone();
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentStateInit {
            model: Some(model),
            system_prompt: Some(system_prompt_text.clone()),
            tools: Some(tool_list),
            thinking_level: Some(thinking_level),
        }),
        convert_to_llm: None,
        transform_context: {
            let cell_for_compact = orch.event_forwarder_cell.clone();
            Some(Arc::new(move |messages, _cancel| {
                let model = model_for_compact.clone();
                let cell = cell_for_compact.clone();
                Box::pin(async move {
                    let before = agent::context::estimate_tokens(&messages) as u64;
                    let result = agent::context::compact_messages(messages, &model);
                    let after = agent::context::estimate_tokens(&result) as u64;
                    if after < before {
                        if let Some(fwd) = cell.lock().ok().and_then(|g| g.clone()) {
                            fwd(agent::types::AgentEvent::ContextCompact {
                                thread_alias: None,
                                before_tokens: before,
                                after_tokens: after,
                                strategy: "mechanical".to_string(),
                            });
                        }
                    }
                    result
                })
            }))
        },
        stream_fn: None,
        steering_mode: None,
        follow_up_mode: None,
        session_id: None,
        get_api_key: Some(get_api_key),
        thinking_budgets: None,
        max_turns,
    });

    // Populate the event forwarder so thread tools can forward inner events
    // to the parent agent's subscribers.
    *orch.event_forwarder_cell.lock().unwrap() = Some(agent.event_forwarder());

    Ok(BuiltAgent {
        agent,
        config,
        model_id,
        model_provider,
        system_prompt_text,
        tool_names,
        skills: loaded_skills.skills,
        permission_service,
        orchestrator,
        startup_messages,
    })
}
