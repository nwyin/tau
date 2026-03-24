use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use agent::stats::AgentStats;
use agent::types::AgentEvent;
use ai::types::AssistantMessageEvent;
use anyhow::Result;
use clap::Parser;

use coding_agent::agent_builder::{build_agent, AgentBuildConfig};
use coding_agent::cli::{Cli, Command};
use coding_agent::session::{SessionFile, SessionManager};
use coding_agent::tools::tools_for_edit_mode;
use coding_agent::trace::{sha256_prefix, TraceConfig, TraceSubscriber};

fn print_models(filter_provider: Option<&str>) {
    ai::register_builtin_providers();

    let mut providers = ai::models::get_providers();
    providers.sort();

    for provider in &providers {
        if let Some(filter) = filter_provider {
            if !provider.contains(filter) {
                continue;
            }
        }

        let mut models = ai::models::get_models(provider);
        models.sort_by(|a, b| a.id.cmp(&b.id));

        if models.is_empty() {
            continue;
        }

        println!("\n  {} ({} models)", provider, models.len());
        println!(
            "  {:<42} {:>7} {:>7}  {:>8}  API",
            "MODEL ID", "$/M IN", "$/M OUT", "CONTEXT"
        );
        println!("  {}", "-".repeat(82));

        for m in &models {
            let ctx = if m.context_window >= 1_000_000 {
                format!("{:.1}M", m.context_window as f64 / 1_000_000.0)
            } else {
                format!("{}K", m.context_window / 1000)
            };
            let reasoning = if m.reasoning { " *" } else { "" };
            println!(
                "  {:<42} {:>7.2} {:>7.2}  {:>8}  {}{}",
                m.id, m.cost.input, m.cost.output, ctx, m.api, reasoning
            );
        }
    }

    println!();
    println!("  * = reasoning model");
    println!("  Set model with: tau -m <MODEL_ID>  or  TAU_MODEL=<MODEL_ID>");
    println!();
}

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

    // Dispatch subcommands
    match cli.command {
        Some(Command::Serve {
            cwd,
            model,
            tools,
            trace_output,
            task_id,
        }) => {
            return coding_agent::serve::run_serve(cwd, model, tools, trace_output, task_id).await;
        }
        Some(Command::Models { provider }) => {
            print_models(provider.as_deref());
            return Ok(());
        }
        None => {}
    }

    // --- Interactive / one-shot mode (existing behavior) ---

    let print_stats = cli.stats;
    let stats_json_path = cli.stats_json.clone();
    let prompt_arg = cli.prompt.clone();
    let trace_output = cli.trace_output.clone();
    let task_id = cli.task_id.clone();

    let built = build_agent(AgentBuildConfig {
        model_id: cli.model,
        system_prompt: cli.system_prompt,
        tools: cli.tools,
        max_turns: None,
        no_skills: cli.no_skills,
        skill_paths: cli.skill_paths,
    })
    .await?;

    let agent = built.agent;
    let config = built.config;
    let skills = built.skills;
    let model_id = built.model_id;
    let model_provider = built.model_provider;
    let system_prompt_hash = sha256_prefix(&built.system_prompt_text);
    let max_turns = std::env::var("TAU_MAX_TURNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .or(config.max_turns);

    // --- Session setup ---
    let session_mgr = SessionManager::new(default_session_dir());

    let (initial_messages, session_file_opt) = if let Some(ref id) = cli.session {
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
    } else {
        // no_session or default: ephemeral (no persistence)
        (vec![], None)
    };

    // Set up stats collection if requested
    let (stats, _stats_unsub) = if print_stats || stats_json_path.is_some() {
        let s = AgentStats::new();
        let unsub = agent.subscribe(s.handler());
        (Some(s), Some(unsub))
    } else {
        (None, None)
    };

    // Set up trace output if requested
    let tool_names: Vec<String> = tools_for_edit_mode(&config.edit_mode)
        .iter()
        .map(|t| t.name().to_string())
        .collect();
    let (_trace, _trace_unsub) = if let Some(ref trace_dir) = trace_output {
        let t = TraceSubscriber::new(
            trace_dir,
            TraceConfig {
                run_id: uuid::Uuid::new_v4().to_string(),
                task_id: task_id.clone(),
                model_id: model_id.clone(),
                provider: model_provider.clone(),
                tool_names,
                edit_mode: config.edit_mode.clone(),
                system_prompt_hash,
                max_turns,
            },
        );
        let unsub = agent.subscribe(t.handler());
        (Some(t), Some(unsub))
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
        if let Some(ref t) = _trace {
            t.finalize();
        }

        let had_error = result.is_err()
            || abort_count.load(Ordering::SeqCst) > 0
            || agent.with_state(|s| {
                s.messages.iter().rev().any(|m| {
                    matches!(
                        m,
                        agent::types::AgentMessage::Llm(ai::types::Message::Assistant(am))
                            if am.stop_reason == ai::types::StopReason::Error
                    )
                })
            });
        let exit_code = if had_error { 1 } else { 0 };

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

            let input = match coding_agent::skills::expand_skill_command(&input, &skills) {
                Some(expanded) => {
                    let name = &input[7..input.find(' ').unwrap_or(input.len())];
                    eprintln!("[skill: {}]", name);
                    expanded
                }
                None => {
                    if input.starts_with("/skill:") {
                        let name = &input[7..input.find(' ').unwrap_or(input.len())];
                        eprintln!(
                            "Unknown skill '{}'. Available: {}",
                            name,
                            if skills.is_empty() {
                                "(none)".to_string()
                            } else {
                                skills
                                    .iter()
                                    .map(|s| s.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            }
                        );
                        continue;
                    }
                    input
                }
            };

            if let Err(e) = agent.prompt(input).await {
                eprintln!("Error: {}", e);
            }
        }
    }

    Ok(())
}
