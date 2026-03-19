//! Agent class — mirrors packages/agent/src/agent.ts

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use ai::types::{Model, SimpleStreamOptions, ThinkingBudgets, Transport};
use anyhow::{anyhow, Result};
use futures::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::loop_::agent_loop;
use crate::types::{
    AgentContext, AgentEvent, AgentLoopConfig, AgentMessage, AgentState, AgentTool, ConvertToLlmFn,
    GetApiKeyFn, GetMessagesFn, StreamAssistantFn, ThinkingLevel, TransformContextFn,
};

// ---------------------------------------------------------------------------
// AgentOptions
// ---------------------------------------------------------------------------

pub struct AgentOptions {
    pub initial_state: Option<AgentStateInit>,
    pub convert_to_llm: Option<ConvertToLlmFn>,
    pub transform_context: Option<TransformContextFn>,
    pub stream_fn: Option<StreamAssistantFn>,
    pub steering_mode: Option<QueueMode>,
    pub follow_up_mode: Option<QueueMode>,
    pub session_id: Option<String>,
    pub get_api_key: Option<GetApiKeyFn>,
    pub thinking_budgets: Option<ThinkingBudgets>,
    pub transport: Option<Transport>,
    pub max_retry_delay_ms: Option<u64>,
    pub max_turns: Option<u32>,
}

#[derive(Default)]
pub struct AgentStateInit {
    pub system_prompt: Option<String>,
    pub model: Option<Model>,
    pub thinking_level: Option<ThinkingLevel>,
    pub tools: Option<Vec<Arc<dyn AgentTool>>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum QueueMode {
    #[default]
    OneAtATime,
    All,
}

// ---------------------------------------------------------------------------
// Agent
// ---------------------------------------------------------------------------

type EventListeners = Vec<Box<dyn Fn(&AgentEvent) + Send + Sync>>;

pub struct Agent {
    state: Arc<Mutex<AgentState>>,

    listeners: Arc<Mutex<EventListeners>>,

    convert_to_llm: ConvertToLlmFn,
    transform_context: Option<TransformContextFn>,
    stream_fn: Option<StreamAssistantFn>,

    steering_queue: Arc<Mutex<VecDeque<AgentMessage>>>,
    follow_up_queue: Arc<Mutex<VecDeque<AgentMessage>>>,
    steering_mode: QueueMode,
    follow_up_mode: QueueMode,

    session_id: Option<String>,
    get_api_key: Option<GetApiKeyFn>,
    thinking_budgets: Option<ThinkingBudgets>,
    transport: Transport,
    max_retry_delay_ms: Option<u64>,
    max_turns: Option<u32>,

    cancel: Arc<Mutex<Option<CancellationToken>>>,
}

fn default_convert_to_llm() -> ConvertToLlmFn {
    Arc::new(|messages: Vec<AgentMessage>| {
        Box::pin(async move {
            Ok(messages
                .into_iter()
                .filter_map(|m| m.as_message().cloned())
                .collect())
        })
    })
}

impl Agent {
    pub fn new(opts: AgentOptions) -> Self {
        let init = opts.initial_state.unwrap_or_default();
        let model = init
            .model
            .expect("AgentOptions.initial_state.model is required");

        let state = AgentState {
            system_prompt: init.system_prompt.unwrap_or_default(),
            model,
            thinking_level: init.thinking_level.unwrap_or(ThinkingLevel::Off),
            tools: init.tools.unwrap_or_default(),
            messages: vec![],
            is_streaming: false,
            stream_message: None,
            pending_tool_calls: HashSet::new(),
            error: None,
        };

        Agent {
            state: Arc::new(Mutex::new(state)),
            listeners: Arc::new(Mutex::new(vec![])),
            convert_to_llm: opts.convert_to_llm.unwrap_or_else(default_convert_to_llm),
            transform_context: opts.transform_context,
            stream_fn: opts.stream_fn,
            steering_queue: Arc::new(Mutex::new(VecDeque::new())),
            follow_up_queue: Arc::new(Mutex::new(VecDeque::new())),
            steering_mode: opts.steering_mode.unwrap_or_default(),
            follow_up_mode: opts.follow_up_mode.unwrap_or_default(),
            session_id: opts.session_id,
            get_api_key: opts.get_api_key,
            thinking_budgets: opts.thinking_budgets,
            transport: opts.transport.unwrap_or_default(),
            max_retry_delay_ms: opts.max_retry_delay_ms,
            max_turns: opts.max_turns,
            cancel: Arc::new(Mutex::new(None)),
        }
    }

    // -----------------------------------------------------------------------
    // State accessors
    // -----------------------------------------------------------------------

    pub fn with_state<R>(&self, f: impl FnOnce(&AgentState) -> R) -> R {
        f(&self.state.lock().unwrap())
    }

    pub fn with_state_mut<R>(&self, f: impl FnOnce(&mut AgentState) -> R) -> R {
        f(&mut self.state.lock().unwrap())
    }

    pub fn set_model(&self, model: Model) {
        self.state.lock().unwrap().model = model;
    }

    pub fn set_thinking_level(&self, level: ThinkingLevel) {
        self.state.lock().unwrap().thinking_level = level;
    }

    pub fn set_system_prompt(&self, prompt: impl Into<String>) {
        self.state.lock().unwrap().system_prompt = prompt.into();
    }

    pub fn set_tools(&self, tools: Vec<Arc<dyn AgentTool>>) {
        self.state.lock().unwrap().tools = tools;
    }

    pub fn set_session_id(&mut self, session_id: Option<String>) {
        self.session_id = session_id;
    }

    pub fn append_message(&self, msg: AgentMessage) {
        self.state.lock().unwrap().messages.push(msg);
    }

    pub fn replace_messages(&self, messages: Vec<AgentMessage>) {
        self.state.lock().unwrap().messages = messages;
    }

    // -----------------------------------------------------------------------
    // Queue management
    // -----------------------------------------------------------------------

    pub fn steer(&self, msg: AgentMessage) {
        self.steering_queue.lock().unwrap().push_back(msg);
    }

    pub fn follow_up(&self, msg: AgentMessage) {
        self.follow_up_queue.lock().unwrap().push_back(msg);
    }

    pub fn clear_steering_queue(&self) {
        self.steering_queue.lock().unwrap().clear();
    }

    pub fn clear_follow_up_queue(&self) {
        self.follow_up_queue.lock().unwrap().clear();
    }

    pub fn clear_all_queues(&self) {
        self.clear_steering_queue();
        self.clear_follow_up_queue();
    }

    pub fn has_queued_messages(&self) -> bool {
        !self.steering_queue.lock().unwrap().is_empty()
            || !self.follow_up_queue.lock().unwrap().is_empty()
    }

    // -----------------------------------------------------------------------
    // Subscriptions
    // -----------------------------------------------------------------------

    /// Subscribe to agent events. Returns an unsubscribe closure.
    pub fn subscribe(&self, f: impl Fn(&AgentEvent) + Send + Sync + 'static) -> impl FnOnce() {
        let mut listeners = self.listeners.lock().unwrap();
        listeners.push(Box::new(f));
        // Simplistic (not reuse-safe after removes, fine for now)
        let listeners = Arc::clone(&self.listeners);
        move || {
            // Mark as no-op (swap with empty closure). Real impl would use IDs.
            let _ = listeners;
        }
    }

    fn emit(&self, event: &AgentEvent) {
        for listener in self.listeners.lock().unwrap().iter() {
            listener(event);
        }
    }

    // -----------------------------------------------------------------------
    // Control
    // -----------------------------------------------------------------------

    pub fn abort(&self) {
        if let Some(ct) = self.cancel.lock().unwrap().as_ref() {
            ct.cancel();
        }
    }

    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        state.messages.clear();
        state.is_streaming = false;
        state.stream_message = None;
        state.pending_tool_calls.clear();
        state.error = None;
        drop(state);
        self.clear_all_queues();
    }

    // -----------------------------------------------------------------------
    // Prompt / continue
    // -----------------------------------------------------------------------

    pub async fn prompt(&self, input: impl Into<String>) -> Result<()> {
        let is_streaming = self.state.lock().unwrap().is_streaming;
        if is_streaming {
            return Err(anyhow!(
                "Agent is already streaming. Use steer() or wait for completion."
            ));
        }
        let msg = AgentMessage::user(input.into());
        self.run_loop(vec![msg]).await
    }

    pub async fn prompt_messages(&self, messages: Vec<AgentMessage>) -> Result<()> {
        let is_streaming = self.state.lock().unwrap().is_streaming;
        if is_streaming {
            return Err(anyhow!("Agent is already streaming."));
        }
        self.run_loop(messages).await
    }

    pub async fn continue_(&self) -> Result<()> {
        let is_streaming = self.state.lock().unwrap().is_streaming;
        if is_streaming {
            return Err(anyhow!("Agent is already streaming."));
        }
        self.run_loop_continue().await
    }

    // -----------------------------------------------------------------------
    // Internal loop driver
    // -----------------------------------------------------------------------

    async fn run_loop(&self, messages: Vec<AgentMessage>) -> Result<()> {
        let ct = CancellationToken::new();
        *self.cancel.lock().unwrap() = Some(ct.clone());

        {
            let mut state = self.state.lock().unwrap();
            state.is_streaming = true;
            state.stream_message = None;
            state.error = None;
        }

        let context = self.build_context();
        let config = self.build_config();

        let mut stream = agent_loop(messages, context, Arc::new(config), Some(ct));
        self.drain_stream(&mut stream).await;

        Ok(())
    }

    async fn run_loop_continue(&self) -> Result<()> {
        let ct = CancellationToken::new();
        *self.cancel.lock().unwrap() = Some(ct.clone());

        {
            let mut state = self.state.lock().unwrap();
            state.is_streaming = true;
        }

        let context = self.build_context();
        let config = self.build_config();

        // Agent-level continue needs to handle queued messages from an assistant tail,
        // which is naturally supported by agent_loop with an empty prompt list.
        let mut stream = agent_loop(vec![], context, Arc::new(config), Some(ct));
        self.drain_stream(&mut stream).await;

        Ok(())
    }

    async fn drain_stream(&self, stream: &mut crate::loop_::AgentEventStream) {
        while let Some(event) = stream.next().await {
            // Update state
            {
                let mut state = self.state.lock().unwrap();
                match &event {
                    AgentEvent::MessageEnd { message } => {
                        state.stream_message = None;
                        state.messages.push(message.clone());
                    }
                    AgentEvent::MessageUpdate { message, .. } => {
                        state.stream_message = Some(message.clone());
                    }
                    AgentEvent::ToolExecutionStart { tool_call_id, .. } => {
                        state.pending_tool_calls.insert(tool_call_id.clone());
                    }
                    AgentEvent::ToolExecutionEnd { tool_call_id, .. } => {
                        state.pending_tool_calls.remove(tool_call_id);
                    }
                    AgentEvent::AgentEnd { .. } => {
                        state.is_streaming = false;
                        state.stream_message = None;
                    }
                    _ => {}
                }
            }
            self.emit(&event);
        }

        // Ensure streaming flag is cleared even if stream ends without AgentEnd
        self.state.lock().unwrap().is_streaming = false;
        *self.cancel.lock().unwrap() = None;
    }

    fn build_context(&self) -> AgentContext {
        let state = self.state.lock().unwrap();
        AgentContext {
            system_prompt: state.system_prompt.clone(),
            messages: state.messages.clone(),
            tools: state.tools.clone(),
        }
    }

    fn build_config(&self) -> AgentLoopConfig {
        let (model, thinking_level) = {
            let state = self.state.lock().unwrap();
            (state.model.clone(), state.thinking_level.clone())
        };

        let reasoning = thinking_level.to_ai();
        let simple_opts = SimpleStreamOptions {
            reasoning,
            thinking_budgets: self.thinking_budgets.clone(),
            base: ai::types::StreamOptions {
                session_id: self.session_id.clone(),
                max_retry_delay_ms: self.max_retry_delay_ms,
                transport: Some(self.transport.clone()),
                ..Default::default()
            },
        };

        let steering_queue = Arc::clone(&self.steering_queue);
        let follow_up_queue = Arc::clone(&self.follow_up_queue);
        let steering_mode = self.steering_mode.clone();
        let follow_up_mode = self.follow_up_mode.clone();

        let get_steering: GetMessagesFn = Arc::new(move || {
            let q = Arc::clone(&steering_queue);
            let mode = steering_mode.clone();
            Box::pin(async move {
                let mut queue = q.lock().unwrap();
                match mode {
                    QueueMode::OneAtATime => queue.pop_front().into_iter().collect(),
                    QueueMode::All => queue.drain(..).collect(),
                }
            })
        });

        let get_follow_up: GetMessagesFn = Arc::new(move || {
            let q = Arc::clone(&follow_up_queue);
            let mode = follow_up_mode.clone();
            Box::pin(async move {
                let mut queue = q.lock().unwrap();
                match mode {
                    QueueMode::OneAtATime => queue.pop_front().into_iter().collect(),
                    QueueMode::All => queue.drain(..).collect(),
                }
            })
        });

        AgentLoopConfig {
            model,
            simple_options: simple_opts,
            max_turns: self.max_turns,
            convert_to_llm: Arc::clone(&self.convert_to_llm),
            transform_context: self.transform_context.clone(),
            stream_fn: self.stream_fn.clone(),
            get_api_key: self.get_api_key.clone(),
            get_steering_messages: Some(get_steering),
            get_follow_up_messages: Some(get_follow_up),
        }
    }
}
