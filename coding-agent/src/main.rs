use std::io::{self, BufRead, Write};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use agent::types::AgentEvent;
use agent::{Agent, AgentOptions, AgentStateInit};
use ai::types::AssistantMessageEvent;
use anyhow::{anyhow, Result};

use coding_agent::tools::all_tools;

#[tokio::main]
async fn main() -> Result<()> {
    // Check for API key first
    let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| {
        eprintln!("Error: OPENAI_API_KEY environment variable is not set.");
        anyhow!("OPENAI_API_KEY not set")
    })?;

    // Register providers
    ai::register_builtin_providers();

    // Resolve model
    let model_id = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
    let model = ai::models::get_model("openai", &model_id)
        .ok_or_else(|| anyhow!("Model '{}' not found in registry", model_id))?;

    let model = (*model).clone();

    // Build agent
    let tools = all_tools();
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentStateInit {
            model: Some(model),
            system_prompt: Some(
                "You are a coding assistant. You can run bash commands, read files, and write files. Be concise.".to_string(),
            ),
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

    // Subscribe to events
    let _unsubscribe = agent.subscribe(move |event| match event {
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

    Ok(())
}
