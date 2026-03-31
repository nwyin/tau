//! Agent loop — mirrors packages/agent/src/agent-loop.ts

use std::sync::Arc;

use ai::stream::event_stream;
use ai::types::{
    AssistantMessage, ContentBlock, Message, StopReason, ToolResultMessage, UserBlock,
};
use futures::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::types::{
    AgentContext, AgentEvent, AgentLoopConfig, AgentMessage, AgentTool, AgentToolResult,
};

// ---------------------------------------------------------------------------
// Event stream type for agent events
// ---------------------------------------------------------------------------

pub type AgentEventStream = ai::stream::EventStream<AgentEvent>;
pub type AgentEventSender = ai::stream::EventStreamSender<AgentEvent>;

fn is_agent_end(e: &AgentEvent) -> bool {
    matches!(e, AgentEvent::AgentEnd { .. })
}

pub fn agent_event_stream() -> (AgentEventSender, AgentEventStream) {
    event_stream(is_agent_end)
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Start an agent loop with new prompt messages.
pub fn agent_loop(
    prompts: Vec<AgentMessage>,
    context: AgentContext,
    config: Arc<AgentLoopConfig>,
    cancel: Option<CancellationToken>,
) -> AgentEventStream {
    let (mut tx, stream) = agent_event_stream();

    tokio::spawn(async move {
        let mut current_context = AgentContext {
            messages: {
                let mut m = context.messages.clone();
                m.extend(prompts.clone());
                m
            },
            ..context
        };

        let new_messages: Vec<AgentMessage> = prompts.clone();

        tx.push(AgentEvent::AgentStart);
        tx.push(AgentEvent::TurnStart);
        for p in &prompts {
            tx.push(AgentEvent::MessageStart { message: p.clone() });
            tx.push(AgentEvent::MessageEnd { message: p.clone() });
        }

        let mut new_messages = new_messages;
        run_loop(
            &mut current_context,
            &mut new_messages,
            &config,
            cancel.clone(),
            &mut tx,
        )
        .await;
    });

    stream
}

/// Continue an agent loop from existing context (no new prompt).
pub fn agent_loop_continue(
    context: AgentContext,
    config: Arc<AgentLoopConfig>,
    cancel: Option<CancellationToken>,
) -> AgentEventStream {
    assert!(
        !context.messages.is_empty(),
        "Cannot continue: no messages in context"
    );
    assert!(
        context.messages.last().map(|m| m.role()) != Some("assistant"),
        "Cannot continue from role: assistant"
    );

    let (mut tx, stream) = agent_event_stream();

    tokio::spawn(async move {
        let mut current_context = context;
        let mut new_messages = vec![];

        tx.push(AgentEvent::AgentStart);
        tx.push(AgentEvent::TurnStart);

        run_loop(
            &mut current_context,
            &mut new_messages,
            &config,
            cancel,
            &mut tx,
        )
        .await;
    });

    stream
}

// ---------------------------------------------------------------------------
// Core loop logic (shared)
// ---------------------------------------------------------------------------

async fn run_loop(
    context: &mut AgentContext,
    new_messages: &mut Vec<AgentMessage>,
    config: &AgentLoopConfig,
    cancel: Option<CancellationToken>,
    tx: &mut AgentEventSender,
) {
    let mut first_turn = true;
    let mut turn_count: u32 = 0;
    let mut pending: Vec<AgentMessage> = get_steering(config).await;

    'outer: loop {
        let mut has_tool_calls = true;
        let mut steering_after_tools: Option<Vec<AgentMessage>> = None;

        while has_tool_calls || !pending.is_empty() {
            // Check cancellation between turns
            if let Some(ref ct) = cancel {
                if ct.is_cancelled() {
                    tx.push(AgentEvent::AgentEnd {
                        messages: new_messages.clone(),
                    });
                    return;
                }
            }

            if let Some(max) = config.max_turns {
                if turn_count >= max {
                    tx.push(AgentEvent::AgentEnd {
                        messages: new_messages.clone(),
                    });
                    return;
                }
            }
            turn_count += 1;

            if !first_turn {
                tx.push(AgentEvent::TurnStart);
            } else {
                first_turn = false;
            }

            // Inject pending (steering) messages
            if !pending.is_empty() {
                for msg in pending.drain(..) {
                    tx.push(AgentEvent::MessageStart {
                        message: msg.clone(),
                    });
                    tx.push(AgentEvent::MessageEnd {
                        message: msg.clone(),
                    });
                    context.messages.push(msg.clone());
                    new_messages.push(msg);
                }
            }

            // Stream assistant response
            let assistant_msg =
                stream_assistant_response(context, config, cancel.clone(), tx).await;

            new_messages.push(AgentMessage::Llm(Message::Assistant(assistant_msg.clone())));

            // Terminal stop reasons
            if matches!(
                assistant_msg.stop_reason,
                StopReason::Error | StopReason::Aborted
            ) {
                tx.push(AgentEvent::TurnEnd {
                    message: AgentMessage::Llm(Message::Assistant(assistant_msg.clone())),
                    tool_results: vec![],
                });
                tx.push(AgentEvent::AgentEnd {
                    messages: new_messages.clone(),
                });
                return;
            }

            // Execute any tool calls
            let tool_call_blocks: Vec<ContentBlock> = assistant_msg
                .content
                .iter()
                .filter(|b| matches!(b, ContentBlock::ToolCall { .. }))
                .cloned()
                .collect();

            has_tool_calls = !tool_call_blocks.is_empty();

            let mut tool_results = vec![];
            if has_tool_calls {
                let exec =
                    execute_tool_calls(&context.tools, &assistant_msg, cancel.clone(), tx, config)
                        .await;
                tool_results = exec.tool_results;
                steering_after_tools = exec.steering;

                for tr in &tool_results {
                    context
                        .messages
                        .push(AgentMessage::Llm(Message::ToolResult(tr.clone())));
                    new_messages.push(AgentMessage::Llm(Message::ToolResult(tr.clone())));
                }
            }

            tx.push(AgentEvent::TurnEnd {
                message: AgentMessage::Llm(Message::Assistant(assistant_msg)),
                tool_results,
            });

            if let Some(steering) = steering_after_tools.take() {
                if !steering.is_empty() {
                    pending = steering;
                    continue;
                }
            }
            pending = get_steering(config).await;
        }

        // Check for follow-up messages
        let follow_ups = get_follow_up(config).await;
        if !follow_ups.is_empty() {
            pending = follow_ups;
            continue 'outer;
        }
        break;
    }

    tx.push(AgentEvent::AgentEnd {
        messages: new_messages.clone(),
    });
}

// ---------------------------------------------------------------------------
// Stream a single assistant response
// ---------------------------------------------------------------------------

async fn stream_assistant_response(
    context: &mut AgentContext,
    config: &AgentLoopConfig,
    cancel: Option<CancellationToken>,
    tx: &mut AgentEventSender,
) -> AssistantMessage {
    // Transform context (optional)
    let messages = if let Some(transform) = &config.transform_context {
        (transform)(context.messages.clone(), None).await
    } else {
        context.messages.clone()
    };

    // Convert to LLM messages
    let llm_messages = match (config.convert_to_llm)(messages).await {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[agent] convert_to_llm error: {}", e);
            let mut msg = AssistantMessage::zero_usage(
                &config.model.api,
                &config.model.provider,
                &config.model.id,
                StopReason::Error,
            );
            msg.error_message = Some(e.to_string());
            return msg;
        }
    };

    let tools = if context.tools.is_empty() {
        None
    } else {
        Some(
            context
                .tools
                .iter()
                .map(|t| ai::types::Tool {
                    name: t.name().to_string(),
                    description: t.description().to_string(),
                    parameters: t.parameters().clone(),
                })
                .collect(),
        )
    };
    let llm_context = ai::types::Context {
        system_prompt: Some(context.system_prompt.clone()),
        messages: llm_messages,
        tools,
    };

    // Resolve API key
    let api_key = if let Some(get_key) = &config.get_api_key {
        (get_key)(config.model.provider.clone()).await
    } else {
        None
    };

    let mut opts = config.simple_options.clone();
    opts.base.api_key = api_key.or(opts.base.api_key.clone());

    let stream_result = match &config.stream_fn {
        Some(stream_fn) => (stream_fn)(
            config.model.clone(),
            llm_context.clone(),
            Some(opts.clone()),
        ),
        None => ai::stream_simple(&config.model, &llm_context, Some(&opts)),
    };

    let event_stream = match stream_result {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[agent] stream error: {}", e);
            let mut msg = AssistantMessage::zero_usage(
                &config.model.api,
                &config.model.provider,
                &config.model.id,
                StopReason::Error,
            );
            msg.error_message = Some(e.to_string());
            tx.push(AgentEvent::MessageStart {
                message: AgentMessage::Llm(Message::Assistant(msg.clone())),
            });
            tx.push(AgentEvent::MessageEnd {
                message: AgentMessage::Llm(Message::Assistant(msg.clone())),
            });
            return msg;
        }
    };

    // Drive the event stream, checking for cancellation
    let mut partial: Option<AssistantMessage> = None;
    let mut pinned = Box::pin(event_stream);

    loop {
        let event = if let Some(ct) = &cancel {
            tokio::select! {
                biased;
                _ = ct.cancelled() => {
                    // Cancelled — return partial with Aborted stop reason
                    let mut msg = partial.unwrap_or_else(|| {
                        AssistantMessage::zero_usage(
                            &config.model.api,
                            &config.model.provider,
                            &config.model.id,
                            StopReason::Aborted,
                        )
                    });
                    msg.stop_reason = StopReason::Aborted;
                    if let Some(last) = context.messages.last_mut() {
                        *last = AgentMessage::Llm(Message::Assistant(msg.clone()));
                    }
                    tx.push(AgentEvent::MessageEnd {
                        message: AgentMessage::Llm(Message::Assistant(msg.clone())),
                    });
                    return msg;
                }
                event = pinned.next() => event,
            }
        } else {
            pinned.next().await
        };

        let Some(event) = event else { break };

        match &event {
            ai::types::AssistantMessageEvent::Start { partial: p } => {
                partial = Some(p.clone());
                let msg = AgentMessage::Llm(Message::Assistant(p.clone()));
                context.messages.push(msg.clone());
                tx.push(AgentEvent::MessageStart { message: msg });
            }
            ai::types::AssistantMessageEvent::Done { message, .. } => {
                // Replace partial in context with final
                if let Some(last) = context.messages.last_mut() {
                    *last = AgentMessage::Llm(Message::Assistant(message.clone()));
                } else {
                    context
                        .messages
                        .push(AgentMessage::Llm(Message::Assistant(message.clone())));
                }
                tx.push(AgentEvent::MessageEnd {
                    message: AgentMessage::Llm(Message::Assistant(message.clone())),
                });
                return message.clone();
            }
            ai::types::AssistantMessageEvent::Error { error: message, .. } => {
                if let Some(ref err) = message.error_message {
                    eprintln!("[agent] provider error: {}", err);
                }
                // Replace partial in context with final
                if let Some(last) = context.messages.last_mut() {
                    *last = AgentMessage::Llm(Message::Assistant(message.clone()));
                } else {
                    context
                        .messages
                        .push(AgentMessage::Llm(Message::Assistant(message.clone())));
                }
                tx.push(AgentEvent::MessageEnd {
                    message: AgentMessage::Llm(Message::Assistant(message.clone())),
                });
                return message.clone();
            }
            _other => {
                if let Some(partial_msg) = &partial {
                    let amsg = AgentMessage::Llm(Message::Assistant(partial_msg.clone()));
                    if let Some(last) = context.messages.last_mut() {
                        *last = amsg.clone();
                    }
                    tx.push(AgentEvent::MessageUpdate {
                        message: amsg,
                        assistant_event: Box::new(event.clone()),
                    });
                }
            }
        }
    }

    // Fallback: return whatever partial we had (shouldn't normally happen)
    partial.unwrap_or_else(|| {
        AssistantMessage::zero_usage(
            &config.model.api,
            &config.model.provider,
            &config.model.id,
            StopReason::Error,
        )
    })
}

// ---------------------------------------------------------------------------
// Tool execution
// ---------------------------------------------------------------------------

struct ToolExecResult {
    tool_results: Vec<ToolResultMessage>,
    steering: Option<Vec<AgentMessage>>,
}

async fn execute_tool_calls(
    tools: &[Arc<dyn AgentTool>],
    assistant_msg: &AssistantMessage,
    cancel: Option<CancellationToken>,
    tx: &mut AgentEventSender,
    config: &AgentLoopConfig,
) -> ToolExecResult {
    let tool_calls: Vec<_> = assistant_msg
        .content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::ToolCall {
                id,
                name,
                arguments,
                ..
            } => Some((id.clone(), name.clone(), arguments.clone())),
            _ => None,
        })
        .collect();

    // Emit all ToolExecutionStart events up front
    for (call_id, call_name, args) in &tool_calls {
        tx.push(AgentEvent::ToolExecutionStart {
            tool_call_id: call_id.clone(),
            tool_name: call_name.clone(),
            args: serde_json::Value::Object(
                args.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            ),
        });
    }

    // Execute all tool calls concurrently
    let futures: Vec<_> = tool_calls
        .iter()
        .map(|(call_id, call_name, args)| {
            let tool = tools.iter().find(|t| t.name() == call_name).cloned();
            let call_id = call_id.clone();
            let call_name = call_name.clone();
            let args = args.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move {
                let (exec_result, is_error) = match tool {
                    None => (
                        AgentToolResult {
                            content: vec![UserBlock::Text {
                                text: format!("Tool {} not found", call_name),
                            }],
                            details: None,
                        },
                        true,
                    ),
                    Some(t) => {
                        let params = serde_json::Value::Object(
                            args.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                        );
                        match t.execute(call_id.clone(), params, cancel, None).await {
                            Ok(r) => (r, false),
                            Err(e) => (
                                AgentToolResult {
                                    content: vec![UserBlock::Text {
                                        text: e.to_string(),
                                    }],
                                    details: None,
                                },
                                true,
                            ),
                        }
                    }
                };
                (call_id, call_name, exec_result, is_error)
            })
        })
        .collect();

    let completed = futures::future::join_all(futures).await;

    // Emit results in order, preserving original tool call sequence
    let mut results = vec![];
    for join_result in completed {
        let (call_id, call_name, exec_result, is_error) = match join_result {
            Ok(r) => r,
            Err(e) => {
                // JoinError (panic in task) — shouldn't happen but handle gracefully
                eprintln!("[agent] tool task panicked: {}", e);
                continue;
            }
        };

        tx.push(AgentEvent::ToolExecutionEnd {
            tool_call_id: call_id.clone(),
            tool_name: call_name.clone(),
            result: exec_result.clone(),
            is_error,
        });

        let tr = ToolResultMessage {
            role: "toolResult".into(),
            tool_call_id: call_id,
            tool_name: call_name,
            content: exec_result.content,
            details: exec_result.details,
            is_error,
            timestamp: chrono::Utc::now().timestamp_millis(),
        };

        results.push(tr.clone());
        tx.push(AgentEvent::MessageStart {
            message: AgentMessage::Llm(Message::ToolResult(tr.clone())),
        });
        tx.push(AgentEvent::MessageEnd {
            message: AgentMessage::Llm(Message::ToolResult(tr)),
        });
    }

    // Check steering once after all tools complete
    let steering = {
        let s = get_steering(config).await;
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    };

    ToolExecResult {
        tool_results: results,
        steering,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn get_steering(config: &AgentLoopConfig) -> Vec<AgentMessage> {
    if let Some(f) = &config.get_steering_messages {
        (f)().await
    } else {
        vec![]
    }
}

async fn get_follow_up(config: &AgentLoopConfig) -> Vec<AgentMessage> {
    if let Some(f) = &config.get_follow_up_messages {
        (f)().await
    } else {
        vec![]
    }
}
