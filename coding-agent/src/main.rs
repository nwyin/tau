use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use agent::stats::AgentStats;
use agent::types::AgentEvent;
use agent::{Agent, AgentOptions, AgentStateInit};
use ai::types::AssistantMessageEvent;
use anyhow::{anyhow, Result};
use clap::Parser;

use coding_agent::cli::Cli;
use coding_agent::session::{SessionFile, SessionManager};
use coding_agent::tools::all_tools;

fn emit_stats(stats: Option<&AgentStats>, print_stats: bool, stats_json_path: Option<&str>) {
    if let Some(s) = stats {
        if print_stats {
            eprintln!("\n{}", s.summary());
        }
        if let Some(path) = stats_json_path {
            let json = s.json();
            match std::fs::write(path, json.to_string()) {
                Ok(_) => {}
                Err(e) => eprintln!("Warning: failed to write stats JSON to {}: {}", path, e),
            }
        }
    }
}

fn default_session_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".tau").join("sessions")
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Capture flags before cli fields are moved
    let print_stats = cli.stats;
    let stats_json_path = cli.stats_json.clone();
    let prompt_arg = cli.prompt.clone();

    // Resolve model: --model flag > TAU_MODEL env > default
    let model_id = cli
        .model
        .or_else(|| std::env::var("TAU_MODEL").ok())
        .unwrap_or_else(|| "gpt-4o-mini".to_string());

    let system_prompt = cli.system_prompt.unwrap_or_else(|| {
        "You are a coding assistant. You can run bash commands, read files, and write files. Be concise.".to_string()
    });

    // Register providers and resolve model
    ai::register_builtin_providers();

    let model = ai::models::find_model(&model_id)
        .ok_or_else(|| anyhow!("Model '{}' not found in registry", model_id))?;
    let model = (*model).clone();

    // Resolve API key based on model provider
    let api_key = match model.provider.as_str() {
        "anthropic" => std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
            anyhow!(
                "ANTHROPIC_API_KEY not set (required for model '{}')",
                model_id
            )
        })?,
        _ => std::env::var("OPENAI_API_KEY")
            .map_err(|_| anyhow!("OPENAI_API_KEY not set (required for model '{}')", model_id))?,
    };

    // --- Session setup ---
    let session_mgr = SessionManager::new(default_session_dir());

    // Determine session mode and load any pre-existing messages
    let (initial_messages, session_file_opt) = if let Some(ref id) = cli.session {
        // Resume specific session
        let messages = session_mgr.load(id).map_err(|e| {
            eprintln!("Error: {}", e);
            e
        })?;
        let sf = session_mgr.open(id)?;
        eprintln!(
            "[session] Resuming session {} ({} messages)",
            id,
            messages.len()
        );
        (messages, Some(sf))
    } else if cli.resume {
        // Resume most recent session
        match session_mgr.latest()? {
            Some(id) => {
                let messages = session_mgr.load(&id)?;
                let sf = session_mgr.open(&id)?;
                eprintln!(
                    "[session] Resuming session {} ({} messages)",
                    id,
                    messages.len()
                );
                (messages, Some(sf))
            }
            None => {
                eprintln!("[session] No previous session found, starting fresh");
                (vec![], None)
            }
        }
    } else if cli.no_session {
        // Explicitly ephemeral
        (vec![], None)
    } else {
        // Default: ephemeral (no persistence unless --session/--resume)
        (vec![], None)
    };

    // Build agent
    let tools = all_tools();
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentStateInit {
            model: Some(model),
            system_prompt: Some(system_prompt),
            tools: Some(tools),
            thinking_level: None,
        }),
        convert_to_llm: None,
        transform_context: None,
        stream_fn: None,
        steering_mode: None,
        follow_up_mode: None,
        session_id: None,
        get_api_key: Some(Arc::new(move |_provider| {
            let key = api_key.clone();
            Box::pin(async move { Some(key) })
        })),
        thinking_budgets: None,
        transport: None,
        max_retry_delay_ms: None,
    });

    // Set up stats collection if requested
    let (stats, _stats_unsub) = if print_stats || stats_json_path.is_some() {
        let s = AgentStats::new();
        let unsub = agent.subscribe(s.handler());
        (Some(s), Some(unsub))
    } else {
        (None, None)
    };

    // Load pre-existing messages into agent state
    if !initial_messages.is_empty() {
        agent.replace_messages(initial_messages);
    }

    // Subscribe to events
    let session_file_arc: Option<Arc<SessionFile>> = session_file_opt.map(Arc::new);
    let session_for_save = session_file_arc.clone();

    let _event_handler = agent.subscribe(move |event| match event {
        AgentEvent::MessageUpdate {
            assistant_event, ..
        } => match assistant_event.as_ref() {
            AssistantMessageEvent::TextDelta { delta, .. } => {
                print!("{}", delta);
                let _ = io::stdout().flush();
            }
            AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                eprint!("[thinking] {}", delta);
                let _ = io::stderr().flush();
            }
            _ => {}
        },
        AgentEvent::MessageEnd { message } => {
            if let Some(ref sf) = session_for_save {
                if let Err(e) = sf.append(message) {
                    eprintln!("Warning: failed to save message to session: {}", e);
                }
            }
        }
        AgentEvent::ToolExecutionStart { tool_name, .. } => {
            eprintln!("[tool: {}]", tool_name);
        }
        AgentEvent::ToolExecutionEnd {
            tool_name,
            is_error,
            ..
        } => {
            if *is_error {
                eprintln!("[tool error: {}]", tool_name);
            }
        }
        AgentEvent::AgentEnd { .. } => {
            println!();
        }
        _ => {}
    });

    // Set up Ctrl-C handler
    let agent = Arc::new(agent);
    let agent_clone = Arc::clone(&agent);
    let abort_count = Arc::new(std::sync::atomic::AtomicU8::new(0));
    let abort_count_clone = Arc::clone(&abort_count);

    tokio::spawn(async move {
        loop {
            tokio::signal::ctrl_c().await.ok();
            let count = abort_count_clone.fetch_add(1, Ordering::SeqCst);
            if count == 0 {
                eprintln!("\n^C (press again to exit)");
                agent_clone.abort();
            } else {
                std::process::exit(0);
            }
        }
    });

    if let Some(ref prompt_text_arg) = prompt_arg {
        // Non-interactive mode: resolve prompt, run once, exit
        let prompt_text = coding_agent::resolve_prompt_text(prompt_text_arg)?;

        let result = agent.prompt(prompt_text).await;

        // Emit stats after run
        emit_stats(stats.as_ref(), print_stats, stats_json_path.as_deref());

        let exit_code = if result.is_err() || abort_count.load(Ordering::SeqCst) > 0 {
            1
        } else {
            0
        };

        std::process::exit(exit_code);
    } else {
        // REPL loop
        let stdin = io::stdin();
        loop {
            print!("> ");
            io::stdout().flush()?;

            let mut line = String::new();
            match stdin.lock().read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Error reading input: {}", e);
                    break;
                }
            }

            let input = line.trim().to_string();
            if input.is_empty() {
                continue;
            }

            abort_count.store(0, Ordering::SeqCst);

            if let Err(e) = agent.prompt(input).await {
                eprintln!("Error: {}", e);
            }
        }
    }

    Ok(())
}
