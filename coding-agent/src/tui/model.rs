use std::collections::HashMap;
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
use super::panes::{
    ChatAction, ChatPane, EditorPane, SidebarAction, SidebarData, SidebarPane, SidebarThreadData,
    ThreadModalPane,
};
use super::sidebar::SidebarThreadStatus;
use super::status::{self, FocusHint};
use super::theme;
use crate::permissions::{PermissionService, PromptResult};
use crate::session::{SessionFile, SessionManager};
use crate::skills::{self, Skill};

// ---------------------------------------------------------------------------
// Screen state
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Landing,
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
// Thread entries
// ---------------------------------------------------------------------------

struct ThreadEntry {
    thread_id: String,
    alias: String,
    task: String,
    model: String,
    status: ThreadEntryStatus,
}

#[derive(Clone, Copy, PartialEq)]
enum ThreadEntryStatus {
    Running,
    Completed,
    Failed,
}

impl ThreadEntryStatus {
    fn to_sidebar_status(self) -> SidebarThreadStatus {
        match self {
            Self::Running => SidebarThreadStatus::Running,
            Self::Completed => SidebarThreadStatus::Completed,
            Self::Failed => SidebarThreadStatus::Failed,
        }
    }
}

// ---------------------------------------------------------------------------
// Pending permission
// ---------------------------------------------------------------------------

struct PendingPermission {
    tool_name: String,
    description: String,
    resp_tx: std::sync::mpsc::Sender<PromptResult>,
}

// ---------------------------------------------------------------------------
// Configuration
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

// ---------------------------------------------------------------------------
// TauModel
// ---------------------------------------------------------------------------

pub struct TauModel {
    // Scene compositor — manages chat, sidebar, editor, and thread modal panes
    scene: Scene,

    // Dimensions
    width: usize,
    height: usize,
    is_compact: bool,

    // Screen
    screen: Screen,

    // Chat data (canonical source — pushed to ChatPane after mutations)
    messages: Vec<ChatMessage>,
    streaming: Option<StreamingState>,

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

    // Todos
    todos: Vec<crate::tools::TodoItem>,

    // Environment
    cwd: String,

    // Permissions
    permission_service: Arc<PermissionService>,
    perm_queue: std::collections::VecDeque<PendingPermission>,

    // Session
    session_manager: SessionManager,
    #[allow(dead_code)]
    session_file: Option<Arc<SessionFile>>,

    // Skills + state
    skills: Vec<Skill>,
    is_busy: bool,
    #[allow(dead_code)]
    should_quit: bool,
    ctrl_c_count: u8,
    #[allow(dead_code)]
    active_thread_count: usize,
    startup_messages: Vec<String>,
    debug: bool,
    warning: Option<String>,

    // Thread data
    thread_entries: Vec<ThreadEntry>,
    thread_messages: HashMap<String, Vec<ChatMessage>>,
    thread_streaming: HashMap<String, StreamingState>,
}

impl TauModel {
    pub fn new(agent: Arc<Agent>, config: TauConfig) -> Self {
        let mut scene = Scene::new();

        // Chat pane (z=0, visible)
        scene.add(
            "chat",
            ChatPane::new(80, 20),
            PaneLayout::new(Rect::new(0, 0, 80, 20), 0),
        );

        // Sidebar pane (z=0, visible when not compact)
        scene.add(
            "sidebar",
            SidebarPane::new(30, 20),
            PaneLayout::new(Rect::new(50, 0, 30, 20), 0),
        );

        // Editor pane (invisible — rendered manually in view_chat)
        scene.add(
            "editor",
            EditorPane::new(),
            PaneLayout {
                rect: Rect::new(0, 20, 80, 1),
                z: 0,
                visible: false,
            },
        );

        Self {
            scene,
            width: 80,
            height: 24,
            is_compact: false,
            screen: Screen::Landing,
            messages: Vec::new(),
            streaming: None,
            agent,
            model_id: config.model_id,
            context_window: config.context_window,
            tokens_in: 0,
            tokens_out: 0,
            total_cost: 0.0,
            thinking_level: ThinkingLevel::Off,
            active_tools: Vec::new(),
            todos: Vec::new(),
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
            startup_messages: config.startup_messages,
            debug: false,
            warning: None,
            thread_entries: Vec::new(),
            thread_messages: HashMap::new(),
            thread_streaming: HashMap::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Chat content management — renders messages and pushes to ChatPane
    // -----------------------------------------------------------------------

    fn refresh_chat_content(&mut self) {
        let w = self
            .width
            .saturating_sub(if self.is_compact { 0 } else { 30 });
        let chat_focused = self.scene.focused() == Some("chat");
        let selected_msg = self
            .scene
            .pane_as::<ChatPane>("chat")
            .map(|p| p.selected_msg())
            .unwrap_or(0);
        let msg_count = self.messages.len();

        let mut content = String::new();
        for (i, msg) in self.messages.iter().enumerate() {
            if i > 0 {
                content.push('\n');
            }
            let focused = i == selected_msg && chat_focused;
            content.push_str(&msg.render(w, focused));
        }

        if let Some(chat) = self.scene.pane_as_mut::<ChatPane>("chat") {
            chat.set_content(&content, msg_count);
        }
    }

    fn push_system_msg(&mut self, text: &str) {
        self.messages.push(ChatMessage::ToolCall(ToolCallMessage {
            tool_call_id: None,
            tool_name: String::new(),
            header: text.to_string(),
            body: String::new(),
            status: ToolStatus::Success,
            expanded: false,
        }));
        self.refresh_chat_content();
    }

    // -----------------------------------------------------------------------
    // Sidebar data sync — pushes current state to SidebarPane
    // -----------------------------------------------------------------------

    fn sync_sidebar(&mut self) {
        let thinking_str = match self.thinking_level {
            ThinkingLevel::Off => "off",
            ThinkingLevel::Minimal => "minimal",
            ThinkingLevel::Low => "low",
            ThinkingLevel::Medium => "medium",
            ThinkingLevel::High => "high",
            ThinkingLevel::XHigh => "xhigh",
        };
        let threads: Vec<SidebarThreadData> = self
            .thread_entries
            .iter()
            .map(|e| SidebarThreadData {
                alias: e.alias.clone(),
                status: e.status.to_sidebar_status(),
            })
            .collect();
        let data = SidebarData {
            model_id: self.model_id.clone(),
            tokens_in: self.tokens_in,
            tokens_out: self.tokens_out,
            context_window: self.context_window,
            total_cost: self.total_cost,
            thinking_level: thinking_str.to_string(),
            active_tools: self.active_tools.clone(),
            todos: self.todos.clone(),
            cwd: self.cwd.clone(),
            threads,
        };
        if let Some(sidebar) = self.scene.pane_as_mut::<SidebarPane>("sidebar") {
            sidebar.set_data(data);
        }
    }

    // -----------------------------------------------------------------------
    // Thread modal management
    // -----------------------------------------------------------------------

    fn sync_thread_modal(&mut self) {
        if !self.scene.contains("thread_modal") {
            return;
        }
        let thread_id = self
            .scene
            .pane_as::<ThreadModalPane>("thread_modal")
            .map(|p| p.thread_id.clone());
        if let Some(thread_id) = thread_id {
            let lo = layout::compute_layout(self.width, self.height, self.is_compact, 3);
            let modal_w = (lo.chat_w * 80 / 100).max(40);
            let inner_w = modal_w.saturating_sub(4);

            let mut content = String::new();
            if let Some(msgs) = self.thread_messages.get(&thread_id) {
                for (i, msg) in msgs.iter().enumerate() {
                    if i > 0 {
                        content.push('\n');
                    }
                    content.push_str(&msg.render(inner_w, false));
                }
            }

            if let Some(modal) = self.scene.pane_as_mut::<ThreadModalPane>("thread_modal") {
                modal.set_content(&content);
            }
        }
    }

    fn open_thread_modal(&mut self, thread_idx: usize) -> Cmd {
        let entry = &self.thread_entries[thread_idx];
        let lo = layout::compute_layout(self.width, self.height, self.is_compact, 3);
        let modal_w = (lo.chat_w * 80 / 100).max(40);
        let modal_h = (lo.chat_h * 80 / 100).max(10);
        let modal_x = (lo.chat_w.saturating_sub(modal_w)) / 2;
        let modal_y = (lo.chat_h.saturating_sub(modal_h)) / 2;

        let pane = ThreadModalPane::new(
            entry.thread_id.clone(),
            entry.alias.clone(),
            entry.task.clone(),
            entry.model.clone(),
            entry.status.to_sidebar_status(),
            modal_w,
            modal_h,
        );

        let layout = PaneLayout::new(
            Rect::new(
                modal_x as u16,
                modal_y as u16,
                modal_w as u16,
                modal_h as u16,
            ),
            1,
        );

        self.scene.add("thread_modal", pane, layout);
        self.sync_thread_modal();
        self.scene.set_focus("thread_modal")
    }

    fn close_thread_modal(&mut self) -> Cmd {
        self.scene.remove("thread_modal");
        self.scene.set_focus("sidebar")
    }

    // -----------------------------------------------------------------------
    // Layout recomputation
    // -----------------------------------------------------------------------

    fn recompute_layout(&mut self) {
        let lo = layout::compute_layout(self.width, self.height, self.is_compact, 3);
        let chat_h = lo.chat_h as u16;
        let chat_w = lo.chat_w as u16;
        let sidebar_w = lo.sidebar_w as u16;

        // Chat pane
        self.scene
            .set_layout("chat", PaneLayout::new(Rect::new(0, 0, chat_w, chat_h), 0));
        if let Some(chat) = self.scene.pane_as_mut::<ChatPane>("chat") {
            chat.resize(lo.chat_w, lo.chat_h);
        }

        // Sidebar pane (hidden when compact)
        self.scene.set_layout(
            "sidebar",
            PaneLayout {
                rect: Rect::new(chat_w, 0, sidebar_w, chat_h),
                z: 0,
                visible: !self.is_compact,
            },
        );
        if let Some(sidebar) = self.scene.pane_as_mut::<SidebarPane>("sidebar") {
            sidebar.set_size(lo.sidebar_w, lo.chat_h);
        }

        // Editor pane (invisible — rendered manually, but needs width for TextInput)
        if let Some(editor) = self.scene.pane_as_mut::<EditorPane>("editor") {
            editor.set_width(self.width);
        }

        // Thread modal (if open)
        if self.scene.contains("thread_modal") {
            let modal_w = (lo.chat_w * 80 / 100).max(40);
            let modal_h = (lo.chat_h * 80 / 100).max(10);
            let modal_x = (lo.chat_w.saturating_sub(modal_w)) / 2;
            let modal_y = (lo.chat_h.saturating_sub(modal_h)) / 2;
            self.scene.set_layout(
                "thread_modal",
                PaneLayout::new(
                    Rect::new(
                        modal_x as u16,
                        modal_y as u16,
                        modal_w as u16,
                        modal_h as u16,
                    ),
                    1,
                ),
            );
            if let Some(modal) = self.scene.pane_as_mut::<ThreadModalPane>("thread_modal") {
                modal.resize(modal_w, modal_h);
            }
        }

        self.sync_sidebar();
        self.refresh_chat_content();
    }

    // -----------------------------------------------------------------------
    // Focus cycling
    // -----------------------------------------------------------------------

    fn cycle_focus(&mut self) -> Cmd {
        let focused = self.scene.focused().map(|s| s.to_string());
        match focused.as_deref() {
            Some("editor") => {
                // Editor → Chat
                self.scene.set_focus("chat");
                self.refresh_chat_content();
                None
            }
            Some("chat") => {
                // Chat → Sidebar (if not compact and threads exist)
                if !self.is_compact && !self.thread_entries.is_empty() {
                    self.scene.set_focus("sidebar")
                } else {
                    self.reset_to_editor()
                }
            }
            Some("sidebar") => self.reset_to_editor(),
            _ => self.reset_to_editor(),
        }
    }

    fn reset_to_editor(&mut self) -> Cmd {
        if let Some(chat) = self.scene.pane_as_mut::<ChatPane>("chat") {
            chat.set_scroll_follow(true);
            chat.goto_bottom();
        }
        self.refresh_chat_content();
        self.scene.set_focus("editor")
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
                    ("Tab", "Cycle focus (editor/chat/sidebar)"),
                    ("j/k", "Scroll / navigate"),
                    ("J/K", "Jump between messages"),
                    ("Space", "Expand/collapse"),
                    ("Enter", "Inspect thread (sidebar)"),
                    ("Esc", "Close modal / back"),
                ];
                for (key, desc) in keys {
                    text.push_str(&format!("  {:<24} {}\n", key, desc));
                }
                self.push_system_msg(&text);
                Some(None)
            }
            "/clear" => {
                self.messages.clear();
                if let Some(chat) = self.scene.pane_as_mut::<ChatPane>("chat") {
                    chat.set_selected_msg(0);
                    chat.set_scroll_follow(true);
                }
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
                            self.sync_sidebar();
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
                            self.sync_sidebar();
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
        self.sync_sidebar();
    }

    // -----------------------------------------------------------------------
    // Agent event handling — mutates shared state, syncs panes
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
                if let Some(editor) = self.scene.pane_as_mut::<EditorPane>("editor") {
                    editor.tick_spinner();
                }
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
            TauMsg::Warning(msg) => {
                self.warning = Some(msg.clone());
                self.is_busy = false;
                if let Some(editor) = self.scene.pane_as_mut::<EditorPane>("editor") {
                    editor.set_busy(false);
                }
                Some(ruse::runtime::CmdInner::Async(Box::pin(async {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    Msg::custom(TauMsg::ClearWarning)
                })))
            }
            TauMsg::ClearWarning => {
                self.warning = None;
                None
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
                tool_call_id,
                tool_name,
                args,
                thread_alias,
                ..
            } => {
                if tool_name == "thread" {
                    return None;
                }
                let header = extract_tool_detail(tool_name, args);
                let display_name = if let Some(alias) = thread_alias {
                    format!("[{}] {}", alias, tool_name)
                } else {
                    tool_name.clone()
                };
                self.messages.push(ChatMessage::ToolCall(ToolCallMessage {
                    tool_call_id: Some(tool_call_id.clone()),
                    tool_name: display_name,
                    header,
                    body: String::new(),
                    status: ToolStatus::Pending,
                    expanded: false,
                }));
                self.active_tools.push(tool_name.clone());
                self.refresh_chat_content();
                self.sync_sidebar();
                None
            }

            AgentEvent::ToolExecutionEnd {
                tool_call_id,
                tool_name,
                result,
                is_error,
                ..
            } => {
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
                        if tc.tool_call_id.as_deref() == Some(tool_call_id)
                            && tc.status == ToolStatus::Pending
                        {
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
                if tool_name == "todo" {
                    if let Some(details) = &result.details {
                        if let Ok(items) = serde_json::from_value::<Vec<crate::tools::TodoItem>>(
                            details.get("todos").cloned().unwrap_or_default(),
                        ) {
                            self.todos = items;
                        }
                    }
                }
                self.active_tools.retain(|t| t != tool_name);
                self.refresh_chat_content();
                self.sync_sidebar();
                None
            }

            AgentEvent::TurnEnd { message, .. } => {
                if let AgentMessage::Llm(ai::types::Message::Assistant(am)) = message {
                    self.tokens_in += am.usage.input;
                    self.tokens_out += am.usage.output;
                    self.total_cost += am.usage.cost.total;
                }
                if let Some(stream) = self.streaming.take() {
                    if let Some(ChatMessage::Assistant(a)) =
                        self.messages.get_mut(stream.assistant_msg_idx)
                    {
                        a.is_streaming = false;
                        a.rendered_content = Some(ruse::glamour::render_dark(&a.content));
                    }
                    self.refresh_chat_content();
                }
                self.sync_sidebar();
                None
            }

            AgentEvent::ThreadStart {
                thread_id,
                alias,
                task,
                model,
            } => {
                self.active_thread_count += 1;
                let header = format!("{}: {}", alias, task.chars().take(60).collect::<String>());
                self.messages.push(ChatMessage::ToolCall(ToolCallMessage {
                    tool_call_id: None,
                    tool_name: "thread".to_string(),
                    header,
                    body: String::new(),
                    status: ToolStatus::Pending,
                    expanded: false,
                }));
                self.active_tools.push(format!("thread:{}", alias));
                self.thread_entries.push(ThreadEntry {
                    thread_id: thread_id.clone(),
                    alias: alias.clone(),
                    task: task.clone(),
                    model: model.clone(),
                    status: ThreadEntryStatus::Running,
                });
                self.thread_messages.entry(thread_id.clone()).or_default();
                self.refresh_chat_content();
                self.sync_sidebar();
                None
            }

            AgentEvent::ThreadEnd { alias, outcome, .. } => {
                self.active_thread_count = self.active_thread_count.saturating_sub(1);
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
                let new_status = match outcome {
                    agent::thread::ThreadOutcome::Completed { .. } => ThreadEntryStatus::Completed,
                    _ => ThreadEntryStatus::Failed,
                };
                if let Some(entry) = self.thread_entries.iter_mut().find(|e| e.alias == *alias) {
                    entry.status = new_status;
                }
                // Update thread modal status if it's showing this thread
                if self.scene.contains("thread_modal") {
                    if let Some(modal) = self.scene.pane_as_mut::<ThreadModalPane>("thread_modal") {
                        modal.set_status(new_status.to_sidebar_status());
                    }
                }
                self.refresh_chat_content();
                self.sync_sidebar();
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
                if let Some(editor) = self.scene.pane_as_mut::<EditorPane>("editor") {
                    editor.set_busy(false);
                }
                self.active_tools.clear();
                self.refresh_chat_content();
                self.sync_sidebar();
                None
            }

            AgentEvent::ThreadEvent {
                thread_id,
                alias: _,
                event,
            } => {
                self.handle_thread_event(thread_id, event);
                self.sync_thread_modal();
                None
            }

            _ => None,
        }
    }

    // -----------------------------------------------------------------------
    // Thread event handling (for inspector modal)
    // -----------------------------------------------------------------------

    fn handle_thread_event(&mut self, thread_id: &str, event: &AgentEvent) {
        let msgs = self
            .thread_messages
            .entry(thread_id.to_string())
            .or_default();

        match event {
            AgentEvent::MessageUpdate {
                assistant_event, ..
            } => match assistant_event.as_ref() {
                AssistantMessageEvent::TextDelta { delta, .. } => {
                    let stream = self
                        .thread_streaming
                        .entry(thread_id.to_string())
                        .or_insert_with(|| {
                            msgs.push(ChatMessage::Assistant(AssistantMessage {
                                thinking: None,
                                thinking_expanded: false,
                                content: String::new(),
                                rendered_content: None,
                                model_name: String::new(),
                                is_streaming: true,
                            }));
                            StreamingState {
                                assistant_msg_idx: msgs.len() - 1,
                                assistant_buf: String::new(),
                                thinking_buf: String::new(),
                                is_thinking: false,
                            }
                        });
                    stream.assistant_buf.push_str(delta);
                    if let Some(ChatMessage::Assistant(a)) = msgs.get_mut(stream.assistant_msg_idx)
                    {
                        a.content.clone_from(&stream.assistant_buf);
                    }
                }
                AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                    let stream = self
                        .thread_streaming
                        .entry(thread_id.to_string())
                        .or_insert_with(|| {
                            msgs.push(ChatMessage::Assistant(AssistantMessage {
                                thinking: Some(String::new()),
                                thinking_expanded: false,
                                content: String::new(),
                                rendered_content: None,
                                model_name: String::new(),
                                is_streaming: true,
                            }));
                            StreamingState {
                                assistant_msg_idx: msgs.len() - 1,
                                assistant_buf: String::new(),
                                thinking_buf: String::new(),
                                is_thinking: true,
                            }
                        });
                    stream.is_thinking = true;
                    stream.thinking_buf.push_str(delta);
                    if let Some(ChatMessage::Assistant(a)) = msgs.get_mut(stream.assistant_msg_idx)
                    {
                        a.thinking = Some(stream.thinking_buf.clone());
                    }
                }
                _ => {}
            },

            AgentEvent::TurnEnd { .. } => {
                if let Some(stream) = self.thread_streaming.remove(thread_id) {
                    if let Some(ChatMessage::Assistant(a)) = msgs.get_mut(stream.assistant_msg_idx)
                    {
                        a.is_streaming = false;
                        a.rendered_content = Some(ruse::glamour::render_dark(&a.content));
                    }
                }
            }

            AgentEvent::ToolExecutionStart {
                tool_call_id,
                tool_name,
                args,
                ..
            } => {
                let header = extract_tool_detail(tool_name, args);
                msgs.push(ChatMessage::ToolCall(ToolCallMessage {
                    tool_call_id: Some(tool_call_id.clone()),
                    tool_name: tool_name.clone(),
                    header,
                    body: String::new(),
                    status: ToolStatus::Pending,
                    expanded: false,
                }));
            }

            AgentEvent::ToolExecutionEnd {
                tool_call_id,
                result,
                is_error,
                ..
            } => {
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
                for msg in msgs.iter_mut().rev() {
                    if let ChatMessage::ToolCall(tc) = msg {
                        if tc.tool_call_id.as_deref() == Some(tool_call_id)
                            && tc.status == ToolStatus::Pending
                        {
                            tc.status = if *is_error {
                                ToolStatus::Error
                            } else {
                                ToolStatus::Success
                            };
                            tc.body = body_text;
                            break;
                        }
                    }
                }
            }

            AgentEvent::AgentEnd { .. } => {
                if let Some(stream) = self.thread_streaming.remove(thread_id) {
                    if let Some(ChatMessage::Assistant(a)) = msgs.get_mut(stream.assistant_msg_idx)
                    {
                        a.is_streaming = false;
                        a.rendered_content = Some(ruse::glamour::render_dark(&a.content));
                    }
                }
            }

            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // Submit prompt
    // -----------------------------------------------------------------------

    fn submit_prompt(&mut self, raw_input: String) -> Cmd {
        // Transition to chat screen
        if self.screen == Screen::Landing {
            self.screen = Screen::Chat;
            // Push initial slash commands to editor pane
            let cmds = self.all_slash_commands();
            if let Some(editor) = self.scene.pane_as_mut::<EditorPane>("editor") {
                editor.set_slash_commands(cmds);
            }
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
        if let Some(editor) = self.scene.pane_as_mut::<EditorPane>("editor") {
            editor.set_busy(true);
        }
        if let Some(chat) = self.scene.pane_as_mut::<ChatPane>("chat") {
            chat.set_scroll_follow(true);
        }
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
                return Msg::custom(TauMsg::Warning(format!("{}", e)));
            }
            Msg::custom(TauMsg::AgentEvent(AgentEvent::AgentEnd {
                messages: Vec::new(),
            }))
        })));

        ruse::runtime::batch(vec![spinner_cmd, prompt_cmd])
    }

    // -----------------------------------------------------------------------
    // Process pane actions after scene.update()
    // -----------------------------------------------------------------------

    fn process_pane_actions(&mut self) -> Cmd {
        // EditorPane: submit
        let submit_text = self
            .scene
            .pane_as_mut::<EditorPane>("editor")
            .and_then(|e| e.take_submit());
        if let Some(text) = submit_text {
            return self.submit_prompt(text);
        }

        // ChatPane: message selection / expand toggle
        let actions = self
            .scene
            .pane_as_mut::<ChatPane>("chat")
            .map(|c| c.take_actions())
            .unwrap_or_default();
        let mut needs_refresh = false;
        for action in actions {
            match action {
                ChatAction::SelectMsg => {
                    needs_refresh = true;
                }
                ChatAction::ToggleExpand(idx) => {
                    if let Some(ChatMessage::ToolCall(tc)) = self.messages.get_mut(idx) {
                        tc.expanded = !tc.expanded;
                        needs_refresh = true;
                    } else if let Some(ChatMessage::Assistant(a)) = self.messages.get_mut(idx) {
                        a.thinking_expanded = !a.thinking_expanded;
                        needs_refresh = true;
                    }
                }
            }
        }
        if needs_refresh {
            self.refresh_chat_content();
        }

        // SidebarPane: open thread / back
        let sidebar_action = self
            .scene
            .pane_as_mut::<SidebarPane>("sidebar")
            .and_then(|s| s.take_action());
        if let Some(action) = sidebar_action {
            match action {
                SidebarAction::OpenThread(idx) => {
                    if idx < self.thread_entries.len() {
                        return self.open_thread_modal(idx);
                    }
                }
                SidebarAction::Back => {
                    return self.reset_to_editor();
                }
            }
        }

        // ThreadModalPane: close
        let should_close = self
            .scene
            .pane_as::<ThreadModalPane>("thread_modal")
            .map(|m| m.should_close())
            .unwrap_or(false);
        if should_close {
            return self.close_thread_modal();
        }

        None
    }
}

// ---------------------------------------------------------------------------
// Model trait implementation
// ---------------------------------------------------------------------------

impl Model for TauModel {
    fn init(&mut self) -> Cmd {
        // Push initial slash commands to editor pane
        let cmds = self.all_slash_commands();
        if let Some(editor) = self.scene.pane_as_mut::<EditorPane>("editor") {
            editor.set_slash_commands(cmds);
        }
        self.sync_sidebar();
        self.scene.set_focus("editor")
    }

    fn update(&mut self, msg: Msg) -> Cmd {
        // 1. Intercept TauMsg (Custom) before Scene
        if let Some(tau_msg) = msg.downcast_ref::<TauMsg>() {
            return self.handle_tau_msg(tau_msg);
        }

        // 2. Permission interception — handles ALL keys while queue non-empty
        if !self.perm_queue.is_empty() {
            if let Msg::KeyPress(key) = &msg {
                match key.code {
                    KeyCode::Char('a') | KeyCode::Char('y') => {
                        if let Some(perm) = self.perm_queue.pop_front() {
                            let _ = perm.resp_tx.send(PromptResult::Allow);
                        }
                    }
                    KeyCode::Char('s') => {
                        if let Some(perm) = self.perm_queue.pop_front() {
                            let tool = perm.tool_name.clone();
                            let _ = perm.resp_tx.send(PromptResult::AlwaysAllow);
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
                        for perm in self.perm_queue.drain(..) {
                            let _ = perm.resp_tx.send(PromptResult::Deny);
                        }
                        self.agent.abort();
                        self.is_busy = false;
                        if let Some(editor) = self.scene.pane_as_mut::<EditorPane>("editor") {
                            editor.set_busy(false);
                        }
                        self.streaming = None;
                        self.active_tools.clear();
                        self.push_system_msg("^C (aborted)");
                        self.sync_sidebar();
                    }
                    _ => {}
                }
            }
            return None;
        }

        // 3. Global keys — intercepted before Scene
        if let Msg::KeyPress(key) = &msg {
            // Reset ctrl-c counter on any non-ctrl-c key
            if !(key.code == KeyCode::Char('c') && key.modifiers.contains(Modifiers::CTRL)) {
                self.ctrl_c_count = 0;
            }

            match key.code {
                KeyCode::Char('c') if key.modifiers.contains(Modifiers::CTRL) => {
                    if self.is_busy {
                        self.agent.abort();
                        self.is_busy = false;
                        if let Some(editor) = self.scene.pane_as_mut::<EditorPane>("editor") {
                            editor.set_busy(false);
                        }
                        self.streaming = None;
                        self.active_tools.clear();
                        self.push_system_msg("^C (aborted)");
                        self.sync_sidebar();
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
                KeyCode::Tab => {
                    // Tab: slash completion (if editor focused + starts with '/') or focus cycling
                    let should_complete = self.scene.focused() == Some("editor")
                        && self
                            .scene
                            .pane_as::<EditorPane>("editor")
                            .map(|e| !e.is_busy() && e.value().starts_with('/'))
                            .unwrap_or(false);

                    if should_complete {
                        if let Some(editor) = self.scene.pane_as_mut::<EditorPane>("editor") {
                            editor.tab_complete();
                        }
                        return None;
                    }
                    return self.cycle_focus();
                }
                _ => {}
            }
        }

        // 4. WindowSize — recompute layout before scene gets the broadcast
        if let Msg::WindowSize { width, height } = &msg {
            self.width = *width as usize;
            self.height = *height as usize;
            self.is_compact = layout::is_compact(self.width, self.height);
            self.recompute_layout();
            // Fall through to scene.update() for broadcast to panes
        }

        // 5. Route to Scene — sends to focused pane (or broadcasts)
        let cmd = self.scene.update(&msg);

        // 6. Process pane actions
        let action_cmd = self.process_pane_actions();

        ruse::runtime::batch(vec![cmd, action_cmd])
    }

    fn view(&self) -> View {
        match self.screen {
            Screen::Landing => View::new(self.view_landing()).with_alt_screen(),
            Screen::Chat => self.view_chat(),
        }
    }
}

// ---------------------------------------------------------------------------
// View rendering
// ---------------------------------------------------------------------------

impl TauModel {
    fn view_landing(&self) -> String {
        let mut lines = Vec::new();

        let logo = Style::new()
            .foreground(Color::parse(theme::PRIMARY))
            .bold(true)
            .render(&["tau"]);
        lines.push(logo);
        lines.push(theme::half_muted_style().render(&["Terminal AI Assistant"]));
        lines.push(String::new());

        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".to_string());
        lines.push(theme::half_muted_style().render(&[&cwd]));

        let model_line = format!(
            "{} {} ",
            theme::primary_style().render(&[theme::MODEL_ICON]),
            theme::subtle_style().render(&[&self.model_id]),
        );
        lines.push(model_line);
        lines.push(String::new());

        for msg in &self.startup_messages {
            lines.push(theme::half_muted_style().render(&[msg.as_str()]));
        }

        let used = lines.len() + 3;
        let spacer = self.height.saturating_sub(used);
        for _ in 0..spacer {
            lines.push(String::new());
        }

        // Input — use editor pane view
        let editor_view = self
            .scene
            .pane_as::<EditorPane>("editor")
            .map(|e| e.view())
            .unwrap_or_default();
        lines.push(editor_view);

        lines.push(theme::separator(self.width));
        lines
            .push(theme::half_muted_style().render(&["enter send | ctrl+c quit | /help commands"]));

        lines.join("\n")
    }

    fn view_chat(&self) -> View {
        let lo = layout::compute_layout(self.width, self.height, self.is_compact, 3);
        let h = self.height as u16;
        let chat_h = lo.chat_h as u16;
        let w = self.width as u16;

        // Get scene view — includes chat, sidebar (if visible), thread_modal (if open)
        let mut view = self.scene.view();

        // Bottom area: editor pane view (or permission prompt) + status bar
        let input_area = if let Some(perm) = self.perm_queue.front() {
            self.render_permission_prompt(perm, lo.chat_w)
        } else {
            self.scene
                .pane_as::<EditorPane>("editor")
                .map(|e| e.view())
                .unwrap_or_default()
        };

        let focus_hint = if !self.perm_queue.is_empty() {
            FocusHint::Permission
        } else {
            match self.scene.focused() {
                Some("editor") => FocusHint::Editor,
                Some("chat") => FocusHint::Chat,
                Some("sidebar") => FocusHint::Sidebar,
                Some("thread_modal") => FocusHint::ThreadModal,
                _ => FocusHint::Editor,
            }
        };
        let status_bar = status::render_status_bar(self.width, focus_hint, self.warning.as_deref());

        let bottom = format!("{}\n{}", input_area, status_bar);
        let bottom_h = h.saturating_sub(chat_h);
        view.regions
            .push((Rect::new(0, chat_h, w, bottom_h), bottom));

        view.with_alt_screen()
    }

    fn render_permission_prompt(&self, perm: &PendingPermission, chat_w: usize) -> String {
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
            .width(chat_w as u16)
            .render(&[&box_content])
    }
}
