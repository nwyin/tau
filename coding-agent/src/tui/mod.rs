mod anim;
mod bridge;
mod chat;
mod dialog;
mod editor;
mod layout;
mod model;
mod msg;
mod sidebar;
mod status;
mod theme;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use agent::types::AgentEvent;
use agent::Agent;
use anyhow::Result;
use ruse::prelude::*;

use crate::permissions::{PermissionService, PromptResult};
use crate::session::{SessionFile, SessionManager};
use crate::skills::Skill;
use model::{TauConfig, TauModel};
use msg::TauMsg;

/// Configuration passed from main.rs to run the TUI.
pub struct TuiRunConfig {
    pub model_id: String,
    pub context_window: u64,
    pub session_file: Option<Arc<SessionFile>>,
    pub session_manager: SessionManager,
    pub skills: Vec<Skill>,
    pub permission_service: Arc<PermissionService>,
    pub startup_messages: Vec<String>,
}

/// Run the interactive TUI.
pub async fn run(agent: Arc<Agent>, config: TuiRunConfig) -> Result<()> {
    let session_file_for_save = config.session_file.clone();
    let permission_service = Arc::clone(&config.permission_service);

    let model = TauModel::new(
        Arc::clone(&agent),
        TauConfig {
            model_id: config.model_id,
            context_window: config.context_window,
            session_file: config.session_file,
            session_manager: config.session_manager,
            skills: config.skills,
            permission_service: Arc::clone(&config.permission_service),
            startup_messages: config.startup_messages,
        },
    );

    let program = Program::new(model)
        .with_alt_screen()
        .with_mouse(MouseMode::CellMotion);

    let (handle, fut) = program.run_with_handle();

    // Agent event bridge: subscribe to agent events and forward via ProgramHandle
    let handle_for_events = handle.clone();
    let _unsub = agent.subscribe(move |event| {
        // Side-effect: persist messages to session file
        if let AgentEvent::MessageEnd { message, .. } = event {
            if let Some(ref sf) = session_file_for_save {
                let _ = sf.append(message);
            }
        }
        // Forward to TUI
        let _ = handle_for_events.send(Msg::custom(TauMsg::AgentEvent(event.clone())));
    });

    // Permission bridge: forward sync permission requests to TUI as async messages
    let (perm_req_tx, mut perm_req_rx) = tokio::sync::mpsc::unbounded_channel::<(
        String,
        String,
        std::sync::mpsc::Sender<PromptResult>,
    )>();
    let handle_for_perms = handle.clone();
    tokio::spawn(async move {
        while let Some((name, desc, resp_tx)) = perm_req_rx.recv().await {
            let _ = handle_for_perms.send(Msg::custom(TauMsg::PermissionRequest {
                tool_name: name,
                description: desc,
                resp_tx,
            }));
        }
    });

    // Shutdown flag — set when the TUI exits so blocking permission prompts can bail out.
    let shutdown = Arc::new(AtomicBool::new(false));

    // Set the permission prompt function that bridges sync agent thread -> async channel.
    // Uses recv_timeout + shutdown flag to avoid hanging the tokio runtime on exit.
    let shutdown_for_perm = Arc::clone(&shutdown);
    let prompt_fn: crate::permissions::PromptFn = Arc::new(move |tool_name: &str, desc: &str| {
        if shutdown_for_perm.load(Ordering::Relaxed) {
            return PromptResult::Deny;
        }
        let (resp_tx, resp_rx) = std::sync::mpsc::channel();
        if perm_req_tx
            .send((tool_name.to_string(), desc.to_string(), resp_tx))
            .is_err()
        {
            return PromptResult::Deny;
        }
        // Poll with timeout so we detect shutdown and don't block forever.
        loop {
            match resp_rx.recv_timeout(std::time::Duration::from_millis(200)) {
                Ok(result) => return result,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if shutdown_for_perm.load(Ordering::Relaxed) {
                        return PromptResult::Deny;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return PromptResult::Deny;
                }
            }
        }
    });
    permission_service.set_prompt_fn(prompt_fn);

    // Run the program
    fut.await.map_err(|e| anyhow::anyhow!("TUI error: {}", e))?;

    // Signal shutdown and abort the agent so spawned tasks can complete
    // and the tokio runtime can shut down cleanly.
    shutdown.store(true, Ordering::Relaxed);
    agent.abort();

    Ok(())
}
