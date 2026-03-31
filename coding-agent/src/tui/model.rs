use std::sync::Arc;

use agent::types::{AgentEvent, AgentMessage, ThinkingLevel};
use agent::Agent;
use ai::types::AssistantMessageEvent;
use ruse::prelude::*;

use super::anim::GradientSpinner;
use super::chat::tools::extract_tool_detail;
use super::chat::{AssistantMessage, ChatMessage, ToolCallMessage, ToolStatus, UserMessage};
use super::layout;
use super::msg::TauMsg;
use super::sidebar;
use super::status::{self, FocusHint};
use super::theme;
use crate::permissions::{PermissionService, PromptResult};
use crate::session::{SessionFile, SessionManager};
use crate::skills::{self, Skill};

// ---------------------------------------------------------------------------
// Screen & focus state
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Landing,
    Chat,
}

#[derive(Clone, Copy, PartialEq)]
enum FocusState {
    Editor,
    Chat,
}

// ---------------------------------------------------------------------------
// Streaming state
// ---------------------------------------------------------------------------

struct StreamingState {
    assistant_buf: String,
    thinking_buf: String,
    #[allow(dead_code)]
    is_thinking: bool,
    assistant_msg_idx: usize,
}

// ---------------------------------------------------------------------------
// Tab completion
// ---------------------------------------------------------------------------

struct TabState {
    #[allow(dead_code)]
    prefix: String,
    candidates: Vec<String>,
    index: usize,
}

// ---------------------------------------------------------------------------
// TauModel
// ---------------------------------------------------------------------------

pub struct TauModel {
    // Dimensions
    width: usize,
    height: usize,
    is_compact: bool,

    // Screen
    screen: Screen,
    focus: FocusState,

    // Chat
    messages: Vec<ChatMessage>,
    chat_viewport: Viewport,
    selected_msg: usize,
    scroll_follow: bool,

    // Editor
    input: TextInput,
    tab_state: Option<TabState>,

    // Streaming
    streaming: Option<StreamingState>,
    spinner: GradientSpinner,

    // Agent
    agent: Arc<Agent>,

    // Metrics
    model_id: String,
    context_window: u64,
    tokens_in: u64,
    tokens_out: u64,
    total_cost: f64,
    thinking_level: ThinkingLevel,
    active_tools: Vec<String>,

    // Environment
    cwd: String,

    // Permissions — queue handles parallel tool calls
    permission_service: Arc<PermissionService>,
    perm_queue: std::collections::VecDeque<PendingPermission>,

    // Session
    session_manager: SessionManager,
    #[allow(dead_code)]
    session_file: Option<Arc<SessionFile>>,

    // Skills + state
    skills: Vec<Skill>,
    is_busy: bool,
    should_quit: bool,
    ctrl_c_count: u8,
    active_thread_count: usize,
    active_thread_aliases: Vec<String>,
    startup_messages: Vec<String>,
    debug: bool,
}

struct PendingPermission {
    tool_name: String,
    description: String,
    resp_tx: std::sync::mpsc::Sender<PromptResult>,
}

// ---------------------------------------------------------------------------
// Configuration (matches TuiRunConfig)
// ---------------------------------------------------------------------------

pub struct TauConfig {
    pub model_id: String,
    pub context_window: u64,
    pub session_file: Option<Arc<SessionFile>>,
    pub session_manager: SessionManager,
    pub skills: Vec<Skill>,
    pub permission_service: Arc<PermissionService>,
    pub startup_messages: Vec<String>,
}

impl TauModel {
    pub fn new(agent: Arc<Agent>, config: TauConfig) -> Self {
        let input = TextInput::new().with_placeholder("Ready for instructions");

        Self {
            width: 80,
            height: 24,
            is_compact: false,
            screen: Screen::Landing,
            focus: FocusState::Editor,
            messages: Vec::new(),
            chat_viewport: Viewport::new(80, 20),
            selected_msg: 0,
            scroll_follow: true,
            input,
            tab_state: None,
            streaming: None,
            spinner: GradientSpinner::new("Thinking"),
            agent,
            model_id: config.model_id,
            context_window: config.context_window,
            tokens_in: 0,
            tokens_out: 0,
            total_cost: 0.0,
            thinking_level: ThinkingLevel::Off,
            active_tools: Vec::new(),
            cwd: std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".to_string()),
            permission_service: config.permission_service,
            perm_queue: std::collections::VecDeque::new(),
            session_manager: config.session_manager,
            session_file: config.session_file,
            skills: config.skills,
            is_busy: false,
            should_quit: false,
            ctrl_c_count: 0,
            active_thread_count: 0,
            active_thread_aliases: Vec::new(),
            startup_messages: config.startup_messages,
            debug: false,
        }
    }

    // -----------------------------------------------------------------------
    // Chat content management
    // -----------------------------------------------------------------------

    fn refresh_chat_content(&mut self) {
        let w = self
            .width
            .saturating_sub(if self.is_compact { 0 } else { 30 });
        let mut content = String::new();

        for (i, msg) in self.messages.iter().enumerate() {
            if i > 0 {
                content.push('\n');
            }
            let focused = i == self.selected_msg && self.focus == FocusState::Chat;
            content.push_str(&msg.render(w, focused));
        }

        self.chat_viewport.set_content(&content);
        if self.scroll_follow {
            self.chat_viewport.goto_bottom();
        }
    }

    /// Push a system/info message into the chat (styled as tool blurred).
    fn push_system_msg(&mut self, text: &str) {
        self.messages.push(ChatMessage::ToolCall(ToolCallMessage {
            tool_name: String::new(),
            header: text.to_string(),
            body: String::new(),
            status: ToolStatus::Success,
            expanded: false,
        }));
        self.refresh_chat_content();
    }

    // -----------------------------------------------------------------------
    // Slash commands
    // -----------------------------------------------------------------------

    fn all_slash_commands(&self) -> Vec<String> {
        let mut cmds = Vec::new();
        for skill in &self.skills {
            cmds.push(format!("/skill:{}", skill.name));
        }
        cmds.extend([
            "/help".into(),
            "/clear".into(),
            "/model".into(),
            "/thinking".into(),
            "/skills".into(),
            "/compact".into(),
            "/sessions".into(),
            "/resume".into(),
            "/yolo".into(),
            "/debug".into(),
        ]);
        cmds
    }

    /// Handle slash command. Returns:
    /// - Some(None) = handled locally
    /// - Some(Some(text)) = expand to text, send to LLM
    /// - None = not a slash command
    fn handle_slash_command(&mut self, input: &str) -> Option<Option<String>> {
        let input = input.trim();
        if !input.starts_with('/') {
            return None;
        }

        let (cmd, args) = input
            .split_once(' ')
            .map(|(c, a)| (c, a.trim()))
            .unwrap_or((input, ""));

        match cmd {
            "/help" => {
                let commands = [
                    ("/help", "Show this help"),
                    ("/clear", "Clear output"),
                    ("/model <id>", "Switch model"),
                    ("/thinking <level>", "Set thinking: off|low|medium|high"),
                    ("/skills", "List available skills"),
                    ("/compact", "Show token/context stats"),
                    ("/sessions", "List sessions for this directory"),
                    ("/resume [id]", "Resume a session (latest if no id)"),
                    ("/yolo", "Toggle auto-approve all tools"),
                    ("/debug", "Toggle debug logging"),
                    ("/skill:<name>", "Run a skill"),
                ];
                let mut text = String::from("Commands:\n");
                for (name, desc) in commands {
                    text.push_str(&format!("  {:<24} {}\n", name, desc));
                }
                text.push_str("\nKeybindings:\n");
                let keys = [
                    ("Ctrl-T", "Cycle thinking level"),
                    ("Ctrl-C", "Abort / exit"),
                    ("Ctrl-D", "Exit"),
                    ("Tab", "Switch focus (editor/chat)"),
                    ("j/k", "Scroll chat"),
                    ("J/K", "Jump between messages"),
                    ("Space", "Expand/collapse"),
                ];
                for (key, desc) in keys {
                    text.push_str(&format!("  {:<24} {}\n", key, desc));
                }
                self.push_system_msg(&text);
                Some(None)
            }
            "/clear" => {
                self.messages.clear();
                self.selected_msg = 0;
                self.scroll_follow = true;
                self.refresh_chat_content();
                Some(None)
            }
            "/model" => {
                if args.is_empty() {
                    self.push_system_msg(&format!(
                        "Current model: {}\nUsage: /model <model-id>",
                        self.model_id
                    ));
                } else {
                    ai::register_builtin_providers();
                    match ai::models::find_model(args) {
                        Some(model) => {
                            let new_id = model.id.clone();
                            let ctx = model.context_window;
                            self.agent.set_model((*model).clone());
                            self.model_id = new_id.clone();
                            self.context_window = ctx;
                            self.push_system_msg(&format!("[model: {}]", new_id));
                        }
                        None => {
                            self.push_system_msg(&format!("Unknown model '{}'", args));
                        }
                    }
                }
                Some(None)
            }
            "/thinking" => {
                if args.is_empty() {
                    let label = format!("{:?}", self.thinking_level).to_lowercase();
                    self.push_system_msg(&format!(
                        "Current thinking level: {}\nUsage: /thinking <off|low|medium|high>",
                        label
                    ));
                } else {
                    let level: Result<ThinkingLevel, _> =
                        serde_json::from_value(serde_json::Value::String(args.to_string()));
                    match level {
                        Ok(l) => {
                            self.agent.set_thinking_level(l.clone());
                            self.thinking_level = l;
                            let label = format!("{:?}", self.thinking_level).to_lowercase();
                            self.push_system_msg(&format!("[thinking: {}]", label));
                        }
                        Err(_) => {
                            self.push_system_msg(&format!(
                                "Invalid thinking level '{}'. Use: off, low, medium, high, xhigh",
                                args
                            ));
                        }
                    }
                }
                Some(None)
            }
            "/skills" => {
                if self.skills.is_empty() {
                    self.push_system_msg("No skills loaded.");
                } else {
                    let mut text = String::from("Available skills:\n");
                    for s in &self.skills {
                        text.push_str(&format!("  /skill:{:<16} {}\n", s.name, s.description));
                    }
                    self.push_system_msg(&text);
                }
                Some(None)
            }
            "/compact" => {
                let ctx_pct = if self.context_window > 0 {
                    (self.tokens_in + self.tokens_out) as f64 / self.context_window as f64 * 100.0
                } else {
                    0.0
                };
                self.push_system_msg(&format!(
                    "Tokens: {} in, {} out | Context: {:.1}% of {} | Cost: ${:.4}",
                    self.tokens_in, self.tokens_out, ctx_pct, self.context_window, self.total_cost
                ));
                Some(None)
            }
            "/sessions" => {
                let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                match self.session_manager.list_for_cwd(&cwd) {
                    Ok(sessions) if sessions.is_empty() => {
                        self.push_system_msg("No sessions for this directory.");
                    }
                    Ok(sessions) => {
                        let mut text = String::from("Sessions:\n");
                        for (id, ts, count) in sessions.iter().take(10) {
                            let date = ts.split('T').next().unwrap_or(ts);
                            let time = ts
                                .split('T')
                                .nth(1)
                                .and_then(|t| t.split('.').next())
                                .unwrap_or("");
                            text.push_str(&format!(
                                "  {} {} {} ({} msgs)\n",
                                id, date, time, count
                            ));
                        }
                        text.push_str("Use /resume <id> to resume a session.");
                        self.push_system_msg(&text);
                    }
                    Err(e) => {
                        self.push_system_msg(&format!("Error listing sessions: {}", e));
                    }
                }
                Some(None)
            }
            "/resume" => {
                let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                let session_id = if args.is_empty() {
                    match self.session_manager.latest_for_cwd(&cwd) {
                        Ok(Some(id)) => id,
                        Ok(None) => {
                            self.push_system_msg("No sessions found for this directory.");
                            return Some(None);
                        }
                        Err(e) => {
                            self.push_system_msg(&format!("Error: {}", e));
                            return Some(None);
                        }
                    }
                } else {
                    args.to_string()
                };

                match self.session_manager.load(&session_id) {
                    Ok(messages) => {
                        let count = messages.len();
                        self.agent.replace_messages(messages);
                        self.push_system_msg(&format!(
                            "[resumed session {} ({} messages)]",
                            session_id, count
                        ));
                    }
                    Err(e) => {
                        self.push_system_msg(&format!(
                            "Error loading session '{}': {}",
                            session_id, e
                        ));
                    }
                }
                Some(None)
            }
            "/yolo" => {
                let new_state = !self.permission_service.is_yolo();
                self.permission_service.set_yolo(new_state);
                self.push_system_msg(&format!("[yolo: {}]", if new_state { "on" } else { "off" }));
                Some(None)
            }
            "/debug" => {
                self.debug = !self.debug;
                self.push_system_msg(&format!(
                    "[debug: {}]",
                    if self.debug { "on" } else { "off" }
                ));
                Some(None)
            }
            _ if cmd.starts_with("/skill:") => {
                let skill_name = &cmd[7..];
                match skills::expand_skill_command(input, &self.skills) {
                    Some(expanded) => {
                        self.push_system_msg(&format!("[skill: {}]", skill_name));
                        Some(Some(expanded))
                    }
                    None => {
                        self.push_system_msg(&format!("Unknown skill '{}'", skill_name));
                        Some(None)
                    }
                }
            }
            _ => {
                self.push_system_msg(&format!(
                    "Unknown command '{}'. Type /help for available commands.",
                    cmd
                ));
                Some(None)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Tab completion
    // -----------------------------------------------------------------------

    fn tab_complete(&mut self) {
        // If already cycling, advance
        if let Some(ref mut state) = self.tab_state {
            if state.candidates.is_empty() {
                return;
            }
            state.index = (state.index + 1) % state.candidates.len();
            let completed = format!("{} ", state.candidates[state.index]);
            self.input.set_value(&completed);
            return;
        }

        let value = self.input.value().to_string();
        if !value.starts_with('/') || value.contains(' ') {
            return;
        }

        let candidates: Vec<String> = self
            .all_slash_commands()
            .into_iter()
            .filter(|c| c.starts_with(&value))
            .collect();

        if candidates.is_empty() {
            return;
        }

        let first = candidates[0].clone();
        self.tab_state = Some(TabState {
            prefix: value,
            candidates,
            index: 0,
        });
        self.input.set_value(&format!("{} ", first));
    }

    // -----------------------------------------------------------------------
    // Thinking level cycling
    // -----------------------------------------------------------------------

    fn cycle_thinking(&mut self) {
        self.thinking_level = match self.thinking_level {
            ThinkingLevel::Off => ThinkingLevel::Low,
            ThinkingLevel::Minimal | ThinkingLevel::Low => ThinkingLevel::Medium,
            ThinkingLevel::Medium => ThinkingLevel::High,
            ThinkingLevel::High | ThinkingLevel::XHigh => ThinkingLevel::Off,
        };
        self.agent.set_thinking_level(self.thinking_level.clone());
        let label = format!("{:?}", self.thinking_level).to_lowercase();
        self.push_system_msg(&format!("[thinking: {}]", label));
    }

    // -----------------------------------------------------------------------
    // Agent event handling
    // -----------------------------------------------------------------------

    fn handle_tau_msg(&mut self, tau_msg: &TauMsg) -> Cmd {
        match tau_msg {
            TauMsg::AgentEvent(event) => self.handle_agent_event(event),
            TauMsg::PermissionRequest {
                tool_name,
                description,
                resp_tx,
            } => {
                self.perm_queue.push_back(PendingPermission {
                    tool_name: tool_name.clone(),
                    description: description.clone(),
                    resp_tx: resp_tx.clone(),
                });
                None
            }
            TauMsg::SpinnerTick => {
                self.spinner.tick();
                if self.is_busy {
                    self.refresh_chat_content();
                    Some(ruse::runtime::CmdInner::Async(Box::pin(async {
                        tokio::time::sleep(GradientSpinner::tick_duration()).await;
                        Msg::custom(TauMsg::SpinnerTick)
                    })))
                } else {
                    None
                }
            }
        }
    }

    fn handle_agent_event(&mut self, event: &AgentEvent) -> Cmd {
        match event {
            AgentEvent::MessageUpdate {
                assistant_event, ..
            } => {
                match assistant_event.as_ref() {
                    AssistantMessageEvent::TextDelta { delta, .. } => {
                        if self.streaming.is_none() {
                            self.messages.push(ChatMessage::Assistant(AssistantMessage {
                                thinking: None,
                                thinking_expanded: false,
                                content: String::new(),
                                rendered_content: None,
                                model_name: self.model_id.clone(),
                                is_streaming: true,
                            }));
                            self.streaming = Some(StreamingState {
                                assistant_msg_idx: self.messages.len() - 1,
                                assistant_buf: String::new(),
                                thinking_buf: String::new(),
                                is_thinking: false,
                            });
                        }
                        if let Some(ref mut stream) = self.streaming {
                            stream.assistant_buf.push_str(delta);
                            if let Some(ChatMessage::Assistant(a)) =
                                self.messages.get_mut(stream.assistant_msg_idx)
                            {
                                a.content.clone_from(&stream.assistant_buf);
                            }
                        }
                        self.refresh_chat_content();
                    }
                    AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                        if self.streaming.is_none() {
                            self.messages.push(ChatMessage::Assistant(AssistantMessage {
                                thinking: Some(String::new()),
                                thinking_expanded: false,
                                content: String::new(),
                                rendered_content: None,
                                model_name: self.model_id.clone(),
                                is_streaming: true,
                            }));
                            self.streaming = Some(StreamingState {
                                assistant_msg_idx: self.messages.len() - 1,
                                assistant_buf: String::new(),
                                thinking_buf: String::new(),
                                is_thinking: true,
                            });
                        }
                        if let Some(ref mut stream) = self.streaming {
                            stream.is_thinking = true;
                            stream.thinking_buf.push_str(delta);
                            if let Some(ChatMessage::Assistant(a)) =
                                self.messages.get_mut(stream.assistant_msg_idx)
                            {
                                a.thinking = Some(stream.thinking_buf.clone());
                            }
                        }
                        self.refresh_chat_content();
                    }
                    _ => {}
                }
                None
            }

            AgentEvent::ToolExecutionStart {
                tool_name, args, ..
            } => {
                // Skip "thread" tool — ThreadStart handles it with richer info
                if tool_name == "thread" {
                    return None;
                }

                let header = extract_tool_detail(tool_name, args);
                // Prefix with thread alias if inside a thread
                let display_name = if !self.active_thread_aliases.is_empty() {
                    // Use the most recent thread alias as context
                    if let Some(alias) = self.active_thread_aliases.last() {
                        format!("[{}] {}", alias, tool_name)
                    } else {
                        tool_name.clone()
                    }
                } else {
                    tool_name.clone()
                };

                self.messages.push(ChatMessage::ToolCall(ToolCallMessage {
                    tool_name: display_name,
                    header,
                    body: String::new(),
                    status: ToolStatus::Pending,
                    expanded: false,
                }));
                self.active_tools.push(tool_name.clone());
                self.refresh_chat_content();
                None
            }

            AgentEvent::ToolExecutionEnd {
                tool_name,
                result,
                is_error,
                ..
            } => {
                // Skip "thread" tool — ThreadEnd handles it
                if tool_name == "thread" {
                    return None;
                }

                let body_text = result
                    .content
                    .iter()
                    .filter_map(|b| {
                        if let ai::types::UserBlock::Text { text } = b {
                            Some(text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                for msg in self.messages.iter_mut().rev() {
                    if let ChatMessage::ToolCall(tc) = msg {
                        if tc.tool_name == *tool_name && tc.status == ToolStatus::Pending {
                            tc.status = if *is_error {
                                ToolStatus::Error
                            } else {
                                ToolStatus::Success
                            };
                            tc.body = body_text.clone();
                            break;
                        }
                    }
                }
                self.active_tools.retain(|t| t != tool_name);
                self.refresh_chat_content();
                None
            }

            AgentEvent::TurnEnd { message, .. } => {
                if let AgentMessage::Llm(ai::types::Message::Assistant(am)) = message {
                    self.tokens_in += am.usage.input;
                    self.tokens_out += am.usage.output;
                    self.total_cost += am.usage.cost.total;
                }
                None
            }

            AgentEvent::ThreadStart { alias, task, .. } => {
                self.active_thread_count += 1;
                self.active_thread_aliases.push(alias.clone());
                let header = format!("{}: {}", alias, task.chars().take(60).collect::<String>());
                self.messages.push(ChatMessage::ToolCall(ToolCallMessage {
                    tool_name: "thread".to_string(),
                    header,
                    body: String::new(),
                    status: ToolStatus::Pending,
                    expanded: false,
                }));
                self.active_tools.push(format!("thread:{}", alias));
                self.refresh_chat_content();
                None
            }

            AgentEvent::ThreadEnd { alias, outcome, .. } => {
                self.active_thread_count = self.active_thread_count.saturating_sub(1);
                self.active_thread_aliases.retain(|a| a != alias);
                self.active_tools
                    .retain(|t| t != &format!("thread:{}", alias));
                for msg in self.messages.iter_mut().rev() {
                    if let ChatMessage::ToolCall(tc) = msg {
                        if tc.tool_name == "thread" && tc.header.starts_with(&format!("{}:", alias))
                        {
                            tc.status = match outcome {
                                agent::thread::ThreadOutcome::Completed { .. } => {
                                    ToolStatus::Success
                                }
                                _ => ToolStatus::Error,
                            };
                            break;
                        }
                    }
                }
                self.refresh_chat_content();
                None
            }

            AgentEvent::AgentEnd { .. } => {
                if let Some(stream) = self.streaming.take() {
                    if let Some(ChatMessage::Assistant(a)) =
                        self.messages.get_mut(stream.assistant_msg_idx)
                    {
                        a.is_streaming = false;
                        a.rendered_content = Some(ruse::glamour::render_dark(&a.content));
                    }
                }
                self.is_busy = false;
                self.active_tools.clear();
                self.refresh_chat_content();
                None
            }

            _ => None,
        }
    }

    // -----------------------------------------------------------------------
    // Submit prompt
    // -----------------------------------------------------------------------

    fn submit_prompt(&mut self) -> Cmd {
        let raw_input = self.input.value().trim().to_string();
        if raw_input.is_empty() {
            return None;
        }
        self.input.set_value("");
        self.tab_state = None;

        // Transition to chat screen
        if self.screen == Screen::Landing {
            self.screen = Screen::Chat;
        }

        // Try slash command first
        let prompt_text = match self.handle_slash_command(&raw_input) {
            Some(None) => return None,        // handled locally
            Some(Some(expanded)) => expanded, // skill expansion
            None => raw_input.clone(),        // normal prompt
        };

        // Add user message
        self.messages
            .push(ChatMessage::User(UserMessage { text: raw_input }));
        self.is_busy = true;
        self.scroll_follow = true;
        self.refresh_chat_content();

        // Start spinner
        let spinner_cmd: Cmd = Some(ruse::runtime::CmdInner::Async(Box::pin(async {
            tokio::time::sleep(GradientSpinner::tick_duration()).await;
            Msg::custom(TauMsg::SpinnerTick)
        })));

        // Spawn agent prompt
        let agent = Arc::clone(&self.agent);
        let prompt_cmd: Cmd = Some(ruse::runtime::CmdInner::Async(Box::pin(async move {
            if let Err(e) = agent.prompt(prompt_text).await {
                eprintln!("Agent error: {}", e);
            }
            // AgentEnd event will arrive via the bridge — this is a fallback
            Msg::custom(TauMsg::AgentEvent(AgentEvent::AgentEnd {
                messages: Vec::new(),
            }))
        })));

        ruse::runtime::batch(vec![spinner_cmd, prompt_cmd])
    }
}

// ---------------------------------------------------------------------------
// Model trait implementation
// ---------------------------------------------------------------------------

impl Model for TauModel {
    fn init(&mut self) -> Cmd {
        // Focus the input so it accepts keystrokes and shows a cursor
        self.input.focus()
    }

    fn update(&mut self, msg: Msg) -> Cmd {
        // Handle custom TauMsg
        if let Some(tau_msg) = msg.downcast_ref::<TauMsg>() {
            return self.handle_tau_msg(tau_msg);
        }

        // Handle permission input — intercepts all keys while queue is non-empty
        if !self.perm_queue.is_empty() {
            if let Msg::KeyPress(key) = &msg {
                match key.code {
                    KeyCode::Char('a') | KeyCode::Char('y') => {
                        if let Some(perm) = self.perm_queue.pop_front() {
                            let _ = perm.resp_tx.send(PromptResult::Allow);
                        }
                    }
                    KeyCode::Char('s') => {
                        // Always allow: approve this one AND auto-approve remaining
                        // queued permissions for the same tool
                        if let Some(perm) = self.perm_queue.pop_front() {
                            let tool = perm.tool_name.clone();
                            let _ = perm.resp_tx.send(PromptResult::AlwaysAllow);
                            // Auto-approve queued permissions for the same tool
                            let mut remaining = std::collections::VecDeque::new();
                            for p in self.perm_queue.drain(..) {
                                if p.tool_name == tool {
                                    let _ = p.resp_tx.send(PromptResult::Allow);
                                } else {
                                    remaining.push_back(p);
                                }
                            }
                            self.perm_queue = remaining;
                        }
                    }
                    KeyCode::Char('d') | KeyCode::Char('n') | KeyCode::Escape => {
                        if let Some(perm) = self.perm_queue.pop_front() {
                            let _ = perm.resp_tx.send(PromptResult::Deny);
                        }
                    }
                    KeyCode::Char('c') if key.modifiers.contains(Modifiers::CTRL) => {
                        // Ctrl-C during permission: deny ALL and abort
                        for perm in self.perm_queue.drain(..) {
                            let _ = perm.resp_tx.send(PromptResult::Deny);
                        }
                        self.agent.abort();
                        self.is_busy = false;
                        self.streaming = None;
                        self.active_tools.clear();
                        self.push_system_msg("^C (aborted)");
                    }
                    _ => {}
                }
            }
            return None;
        }

        match msg {
            Msg::KeyPress(key) => {
                // Reset ctrl-c counter on any non-ctrl-c key
                if !(key.code == KeyCode::Char('c') && key.modifiers.contains(Modifiers::CTRL)) {
                    self.ctrl_c_count = 0;
                }

                // Global keys
                match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(Modifiers::CTRL) => {
                        if self.is_busy {
                            self.agent.abort();
                            self.is_busy = false;
                            self.streaming = None;
                            self.active_tools.clear();
                            self.push_system_msg("^C (aborted)");
                        } else {
                            self.ctrl_c_count += 1;
                            if self.ctrl_c_count >= 2 {
                                self.should_quit = true;
                                return ruse::runtime::quit();
                            }
                            self.push_system_msg("^C (press again to exit)");
                        }
                        return None;
                    }
                    KeyCode::Char('d') if key.modifiers.contains(Modifiers::CTRL) => {
                        self.should_quit = true;
                        return ruse::runtime::quit();
                    }
                    KeyCode::Char('t') if key.modifiers.contains(Modifiers::CTRL) => {
                        self.cycle_thinking();
                        return None;
                    }
                    KeyCode::Tab if self.focus == FocusState::Editor => {
                        // Tab in editor: try slash command completion first
                        let value = self.input.value().to_string();
                        if value.starts_with('/') {
                            self.tab_complete();
                        } else {
                            self.input.blur();
                            self.focus = FocusState::Chat;
                            self.refresh_chat_content();
                        }
                        return None;
                    }
                    KeyCode::Tab => {
                        self.focus = FocusState::Editor;
                        self.refresh_chat_content();
                        return self.input.focus();
                    }
                    _ => {}
                }

                // Focus-specific keys
                match self.focus {
                    FocusState::Editor => {
                        if self.is_busy {
                            return None;
                        }
                        // Reset tab state on non-tab keys
                        self.tab_state = None;
                        match key.code {
                            KeyCode::Enter => return self.submit_prompt(),
                            _ => {
                                self.input.update(&Msg::KeyPress(key));
                            }
                        }
                    }
                    FocusState::Chat => match key.code {
                        KeyCode::Char('j') | KeyCode::Down => {
                            self.chat_viewport.line_down(1);
                            self.scroll_follow = false;
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            self.chat_viewport.line_up(1);
                            self.scroll_follow = false;
                        }
                        KeyCode::Char('d') => {
                            self.chat_viewport.half_page_down();
                            self.scroll_follow = false;
                        }
                        KeyCode::Char('u') => {
                            self.chat_viewport.half_page_up();
                            self.scroll_follow = false;
                        }
                        KeyCode::Char('G') => {
                            self.chat_viewport.goto_bottom();
                            self.scroll_follow = true;
                        }
                        KeyCode::Char('g') => {
                            self.chat_viewport.goto_top();
                            self.scroll_follow = false;
                        }
                        KeyCode::Char('J') => {
                            if self.selected_msg + 1 < self.messages.len() {
                                self.selected_msg += 1;
                                self.refresh_chat_content();
                            }
                        }
                        KeyCode::Char('K') => {
                            if self.selected_msg > 0 {
                                self.selected_msg -= 1;
                                self.refresh_chat_content();
                            }
                        }
                        KeyCode::Char(' ') => {
                            if let Some(ChatMessage::ToolCall(tc)) =
                                self.messages.get_mut(self.selected_msg)
                            {
                                tc.expanded = !tc.expanded;
                                self.refresh_chat_content();
                            } else if let Some(ChatMessage::Assistant(a)) =
                                self.messages.get_mut(self.selected_msg)
                            {
                                a.thinking_expanded = !a.thinking_expanded;
                                self.refresh_chat_content();
                            }
                        }
                        _ => {}
                    },
                }
                None
            }

            Msg::MouseWheel(mouse) => {
                match mouse.button {
                    ruse::runtime::MouseButton::WheelUp => {
                        self.chat_viewport.line_up(3);
                        self.scroll_follow = false;
                    }
                    ruse::runtime::MouseButton::WheelDown => {
                        self.chat_viewport.line_down(3);
                    }
                    _ => {}
                }
                None
            }

            Msg::WindowSize { width, height } => {
                self.width = width as usize;
                self.height = height as usize;
                self.is_compact = layout::is_compact(self.width, self.height);
                let lo = layout::compute_layout(self.width, self.height, self.is_compact, 3);
                // Resize viewport without losing content
                self.chat_viewport.set_width(lo.chat_w);
                self.chat_viewport.set_height(lo.chat_h);
                self.refresh_chat_content();
                None
            }

            _ => None,
        }
    }

    fn view(&self) -> View {
        let content = match self.screen {
            Screen::Landing => self.view_landing(),
            Screen::Chat => self.view_chat(),
        };

        View::new(content).with_alt_screen()
    }
}

// ---------------------------------------------------------------------------
// View rendering
// ---------------------------------------------------------------------------

impl TauModel {
    fn view_landing(&self) -> String {
        let mut lines = Vec::new();

        // Logo
        let logo = Style::new()
            .foreground(Color::parse(theme::PRIMARY))
            .bold(true)
            .render(&["tau"]);
        lines.push(logo);
        lines.push(theme::half_muted_style().render(&["Terminal AI Assistant"]));
        lines.push(String::new());

        // CWD
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".to_string());
        lines.push(theme::half_muted_style().render(&[&cwd]));

        // Model
        let model_line = format!(
            "{} {} ",
            theme::primary_style().render(&[theme::MODEL_ICON]),
            theme::subtle_style().render(&[&self.model_id]),
        );
        lines.push(model_line);
        lines.push(String::new());

        // Startup messages
        for msg in &self.startup_messages {
            lines.push(theme::half_muted_style().render(&[msg.as_str()]));
        }

        // Spacer to push input to bottom
        let used = lines.len() + 3; // input + separator + help
        let spacer = self.height.saturating_sub(used);
        for _ in 0..spacer {
            lines.push(String::new());
        }

        // Input
        let prompt = format!("{} ", theme::primary_style().render(&[">"]),);
        lines.push(format!("{}{}", prompt, self.input.view()));

        // Separator
        lines.push(theme::separator(self.width));

        // Help
        lines
            .push(theme::half_muted_style().render(&["enter send | ctrl+c quit | /help commands"]));

        lines.join("\n")
    }

    fn view_chat(&self) -> String {
        let lo = layout::compute_layout(self.width, self.height, self.is_compact, 3);

        // Chat viewport
        let chat = self.chat_viewport.view();

        // Input area
        let input_area = if let Some(perm) = self.perm_queue.front() {
            // Permission modal — show front of queue with count
            let count_hint = if self.perm_queue.len() > 1 {
                format!(" (+{})", self.perm_queue.len() - 1)
            } else {
                String::new()
            };
            let header = format!(
                "{}  {} {}{}",
                theme::green_dark_style().render(&[theme::TOOL_PENDING]),
                theme::subtle_style().render(&[&perm.tool_name]),
                theme::half_muted_style().render(&[&perm.description]),
                theme::half_muted_style().render(&[&count_hint]),
            );
            let options = theme::half_muted_style().render(&["[a]llow  [s]ession  [d]eny"]);
            let box_content = format!("{}\n{}", header, options);
            Style::new()
                .border(ROUNDED_BORDER, &[true])
                .border_foreground(Color::parse(theme::GREEN_DARK))
                .padding(&[0, 1])
                .width(lo.chat_w as u16)
                .render(&[&box_content])
        } else if self.is_busy {
            // Spinner
            format!("  {}", self.spinner.view())
        } else {
            // Normal input — simple prompt, model info is in sidebar
            let prompt = format!("  {} ", theme::primary_style().render(&[">"]),);
            format!("{}{}", prompt, self.input.view())
        };

        // Status bar
        let focus_hint = if !self.perm_queue.is_empty() {
            FocusHint::Permission
        } else {
            match self.focus {
                FocusState::Editor => FocusHint::Editor,
                FocusState::Chat => FocusHint::Chat,
            }
        };
        let status_bar = status::render_status_bar(self.width, focus_hint);

        // Compose: manually join chat lines with sidebar lines.
        // This avoids join_horizontal's ANSI width issues and ensures the
        // sidebar renders independently of chat scroll position.
        if self.is_compact {
            format!("{}\n{}\n{}", chat, input_area, status_bar)
        } else {
            let thinking_str = match self.thinking_level {
                ThinkingLevel::Off => "off",
                ThinkingLevel::Minimal => "minimal",
                ThinkingLevel::Low => "low",
                ThinkingLevel::Medium => "medium",
                ThinkingLevel::High => "high",
                ThinkingLevel::XHigh => "xhigh",
            };
            let sb = sidebar::render_sidebar(&sidebar::SidebarState {
                width: lo.sidebar_w,
                height: lo.chat_h,
                model_id: &self.model_id,
                tokens_in: self.tokens_in,
                tokens_out: self.tokens_out,
                context_window: self.context_window,
                total_cost: self.total_cost,
                thinking_level: thinking_str,
                active_tools: &self.active_tools,
                cwd: &self.cwd,
            });

            // Line-by-line composition: pad each chat line to exact chat_w,
            // then append the corresponding sidebar line.
            let chat_lines: Vec<&str> = chat.lines().collect();
            let sb_lines: Vec<&str> = sb.lines().collect();
            let max_lines = chat_lines.len().max(sb_lines.len());
            let mut combined = String::new();
            for i in 0..max_lines {
                if i > 0 {
                    combined.push('\n');
                }
                let cl = chat_lines.get(i).copied().unwrap_or("");
                let sl = sb_lines.get(i).copied().unwrap_or("");
                // Pad chat line to exact pixel width so sidebar stays aligned
                let cl_w = ruse::ansi::string_width(cl);
                if cl_w < lo.chat_w {
                    combined.push_str(cl);
                    for _ in 0..(lo.chat_w - cl_w) {
                        combined.push(' ');
                    }
                } else {
                    combined.push_str(&ruse::ansi::truncate(cl, lo.chat_w, ""));
                }
                combined.push_str(sl);
            }

            format!("{}\n{}\n{}", combined, input_area, status_bar)
        }
    }
}
