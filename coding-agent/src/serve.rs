//! Serve mode: JSON-RPC server over stdio for orchestrator integration.
//!
//! Spawned as `tau serve --cwd <worktree>`. One process per session.
//! Hive (or any orchestrator) writes JSON-RPC requests to stdin and reads
//! responses + notifications from stdout.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;

use crate::agent_builder::{build_agent, AgentBuildConfig};
use crate::rpc::handler::{handle_request, usage_tracking_subscriber, ServerState, SessionStatus};
use crate::rpc::transport::{spawn_stdin_reader, StdinMessage, StdoutWriter};
use crate::rpc::types::*;
use crate::tools::tools_for_edit_mode;
use crate::trace::{sha256_prefix, TraceConfig, TraceSubscriber};

/// Run the JSON-RPC serve loop.
pub async fn run_serve(
    cwd: String,
    model: Option<String>,
    tools: Option<Vec<String>>,
    trace_output: Option<String>,
    task_id: Option<String>,
) -> Result<()> {
    // Set working directory for this session
    std::env::set_current_dir(&cwd)?;
    eprintln!("[serve] starting in {}", cwd);

    // Build agent
    let built = build_agent(AgentBuildConfig {
        model_id: model,
        system_prompt: None,
        tools,
        max_turns: None,
        yolo: true, // serve mode: no interactive prompts
        permission_prompt_fn: None,
        no_skills: false,
        skill_paths: vec![],
    })
    .await?;

    let writer = StdoutWriter::new();
    let cumulative_usage = Arc::new(Mutex::new(UsageReport::default()));

    // Subscribe for usage tracking
    let _usage_unsub = built
        .agent
        .subscribe(usage_tracking_subscriber(Arc::clone(&cumulative_usage)));

    // Set up trace output if requested
    let tool_names: Vec<String> = tools_for_edit_mode(&built.config.edit_mode)
        .iter()
        .map(|t| t.name().to_string())
        .collect();
    let system_prompt_hash = sha256_prefix(&built.system_prompt_text);
    let max_turns = std::env::var("TAU_MAX_TURNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .or(built.config.max_turns);
    let (_trace, _trace_unsub) = if let Some(ref trace_dir) = trace_output {
        let t = TraceSubscriber::new(
            trace_dir,
            TraceConfig {
                run_id: uuid::Uuid::new_v4().to_string(),
                task_id: task_id.clone(),
                model_id: built.model_id.clone(),
                provider: built.model_provider.clone(),
                tool_names,
                edit_mode: built.config.edit_mode.clone(),
                system_prompt_hash,
                max_turns,
            },
        );
        let unsub = built.agent.subscribe(t.handler());
        (Some(t), Some(unsub))
    } else {
        (None, None)
    };

    // Build server state
    let state = Arc::new(ServerState {
        agent: built.agent,
        status: Mutex::new(SessionStatus::Idle),
        writer: writer.clone(),
        cumulative_usage,
        agent_task: Mutex::new(None),
        shutdown: std::sync::atomic::AtomicBool::new(false),
    });

    // Spawn stdin reader
    let mut requests = spawn_stdin_reader();

    // Signal handler
    let state_for_signal = Arc::clone(&state);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        eprintln!("[serve] received signal, shutting down");
        state_for_signal.shutdown.store(true, Ordering::SeqCst);
        state_for_signal.agent.abort();
    });

    eprintln!("[serve] ready");

    // Main request loop
    loop {
        tokio::select! {
            msg = requests.recv() => {
                match msg {
                    Some(StdinMessage::Request(req)) => {
                        handle_request(&state, req).await;
                        if state.shutdown.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                    Some(StdinMessage::ParseError(err)) => {
                        // Can't determine request id on parse error
                        let resp = JsonRpcResponse::error(
                            serde_json::Value::Null,
                            JsonRpcError::new(PARSE_ERROR, err),
                        );
                        writer.write_response(&resp);
                    }
                    None => {
                        // stdin closed
                        eprintln!("[serve] stdin closed, shutting down");
                        break;
                    }
                }
            }
        }

        if state.shutdown.load(Ordering::SeqCst) {
            break;
        }
    }

    // Graceful drain
    state.agent.abort();
    let handle = state.agent_task.lock().unwrap().take();
    if let Some(handle) = handle {
        eprintln!("[serve] waiting for agent task to finish...");
        let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
    }

    // Finalize trace before exit
    if let Some(ref t) = _trace {
        t.finalize();
    }

    eprintln!("[serve] shutdown complete");
    // Force exit — the blocking stdin reader thread would otherwise prevent
    // the tokio runtime from shutting down cleanly.
    std::process::exit(0);
}
