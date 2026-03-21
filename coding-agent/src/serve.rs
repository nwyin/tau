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

/// Run the JSON-RPC serve loop.
pub async fn run_serve(
    cwd: String,
    model: Option<String>,
    tools: Option<Vec<String>>,
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
    })
    .await?;

    let writer = StdoutWriter::new();
    let cumulative_usage = Arc::new(Mutex::new(UsageReport::default()));

    // Subscribe for usage tracking
    let _usage_unsub = built
        .agent
        .subscribe(usage_tracking_subscriber(Arc::clone(&cumulative_usage)));

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

    eprintln!("[serve] shutdown complete");
    Ok(())
}
