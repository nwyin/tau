//! Ratatui-based terminal UI for interactive mode.
//!
//! Replaces the bare `print!("> ")` REPL with a full TUI featuring:
//! - Scrollable output area with colored text
//! - Input line with model-name prompt
//! - Status bar showing model, tokens, context %, cost, active tool

use std::io;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use agent::types::{AgentEvent, AgentMessage, ThinkingLevel};
use agent::Agent;
use ai::types::{AssistantMessageEvent, Message};
use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use futures::StreamExt;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Terminal;

use crate::permissions::{self, PermissionService};
use crate::session::SessionFile;
use crate::skills::{self, Skill};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Configuration passed from main.rs to run the TUI.
pub struct TuiRunConfig {
    pub model_id: String,
    pub context_window: u64,
    pub session_file: Option<Arc<SessionFile>>,
    pub skills: Vec<Skill>,
    pub permission_service: Arc<PermissionService>,
    pub startup_messages: Vec<String>,
}

/// Run the interactive TUI. Enters alternate screen, runs the event loop,
/// and restores the terminal on exit.
pub async fn run(agent: Arc<Agent>, config: TuiRunConfig) -> Result<()> {
    // Setup terminal
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(io::stderr(), EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)?;

    // Panic hook to restore terminal on crash
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(io::stderr(), LeaveAlternateScreen);
        original_hook(info);
    }));

    let result = run_app(&mut terminal, agent, config).await;

    // Teardown
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(io::stderr(), LeaveAlternateScreen)?;

    result
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

struct App {
    // Output
    output_lines: Vec<Line<'static>>,
    scroll_offset: u16,
    auto_scroll: bool,
    streaming_text: String,
    is_thinking: bool,

    // Input
    input: String,
    cursor_pos: usize,
    /// Tab completion state: (prefix being completed, matching candidates, current index)
    tab_state: Option<(String, Vec<String>, usize)>,

    // Status
    model_id: String,
    context_window: u64,
    tokens_in: u64,
    tokens_out: u64,
    total_cost: f64,
    active_tools: Vec<String>,
    thinking_level: ThinkingLevel,
    is_busy: bool,

    // Control
    skills: Vec<Skill>,
    abort_count: Arc<AtomicU8>,
    should_quit: bool,
    debug: bool,
}

impl App {
    fn new(config: &TuiRunConfig) -> Self {
        Self {
            output_lines: vec![],
            scroll_offset: 0,
            auto_scroll: true,
            streaming_text: String::new(),
            is_thinking: false,

            input: String::new(),
            cursor_pos: 0,
            tab_state: None,

            model_id: config.model_id.clone(),
            context_window: config.context_window,
            tokens_in: 0,
            tokens_out: 0,
            total_cost: 0.0,
            active_tools: vec![],
            thinking_level: ThinkingLevel::Off,
            is_busy: false,

            skills: config.skills.clone(),
            abort_count: Arc::new(AtomicU8::new(0)),
            should_quit: false,
            debug: false,
        }
    }

    fn debug_log(&mut self, msg: impl Into<String>) {
        if self.debug {
            self.push_line(Line::from(Span::styled(
                format!("[debug] {}", msg.into()),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    /// Reset tab completion state (called when input changes via non-Tab keys).
    fn reset_tab(&mut self) {
        self.tab_state = None;
    }

    /// Get all completable slash commands (built-in + skills).
    fn all_slash_commands(&self) -> Vec<String> {
        let mut cmds = vec![
            "/help".to_string(),
            "/clear".to_string(),
            "/model".to_string(),
            "/thinking".to_string(),
            "/skills".to_string(),
            "/compact".to_string(),
            "/debug".to_string(),
        ];
        for skill in &self.skills {
            cmds.push(format!("/skill:{}", skill.name));
        }
        cmds
    }

    /// Handle Tab press: complete any `/` command.
    fn tab_complete(&mut self) {
        self.debug_log(format!(
            "tab_complete: input={:?} tab_state={}",
            self.input,
            if self.tab_state.is_some() {
                "Some"
            } else {
                "None"
            }
        ));

        // If already cycling, advance to next candidate
        if let Some((ref prefix, ref candidates, ref mut idx)) = self.tab_state {
            if candidates.is_empty() {
                return;
            }
            *idx = (*idx + 1) % candidates.len();
            self.input = format!("{} ", candidates[*idx]);
            self.cursor_pos = self.input.len();
            let _ = prefix;
            // Can't call debug_log here (borrow conflict), will log after
            return;
        }

        // Need a `/` prefix to trigger completion
        if !self.input.starts_with('/') {
            self.debug_log("tab_complete: no / prefix, skipping");
            return;
        }

        // Don't complete if there's already a space (user is typing args)
        if self.input.contains(' ') {
            self.debug_log("tab_complete: has space, skipping");
            return;
        }

        let partial = self.input.clone();
        let candidates: Vec<String> = self
            .all_slash_commands()
            .into_iter()
            .filter(|c| c.starts_with(&partial))
            .collect();

        self.debug_log(format!(
            "tab_complete: partial={:?} candidates={:?}",
            partial, candidates
        ));

        if candidates.is_empty() {
            return;
        }

        if candidates.len() == 1 {
            self.input = format!("{} ", candidates[0]);
            self.cursor_pos = self.input.len();
            self.debug_log(format!("tab_complete: single match -> {:?}", self.input));
        } else {
            let first = candidates[0].clone();
            self.tab_state = Some((partial, candidates, 0));
            self.input = format!("{} ", first);
            self.cursor_pos = self.input.len();
            self.debug_log(format!(
                "tab_complete: cycling mode, first -> {:?}",
                self.input
            ));
        }
    }

    /// Try to handle a slash command. Returns:
    /// - Some(None) = handled locally, don't send to LLM
    /// - Some(Some(text)) = expand to this text, send to LLM
    /// - None = not a slash command, send input as-is to LLM
    fn handle_slash_command(&mut self, input: &str, agent: &Agent) -> Option<Option<String>> {
        let input = input.trim();
        if !input.starts_with('/') {
            return None;
        }

        let (cmd, args) = input
            .split_once(' ')
            .map(|(c, a)| (c, a.trim()))
            .unwrap_or((input, ""));

        self.debug_log(format!(
            "slash_command: input={:?} cmd={:?} args={:?}",
            input, cmd, args
        ));

        match cmd {
            "/help" => {
                self.push_line(Line::from(Span::styled(
                    "Commands:",
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                let commands = [
                    ("/help", "Show this help"),
                    ("/clear", "Clear output"),
                    ("/model <id>", "Switch model"),
                    ("/thinking <level>", "Set thinking: off|low|medium|high"),
                    ("/skills", "List available skills"),
                    ("/compact", "Show token/context stats"),
                    ("/debug", "Toggle debug logging"),
                    ("/skill:<name> [args]", "Run a skill"),
                ];
                for (name, desc) in commands {
                    self.push_line(Line::from(vec![
                        Span::styled(format!("  {:<24}", name), Style::default().fg(Color::Cyan)),
                        Span::styled(desc.to_string(), Style::default().fg(Color::DarkGray)),
                    ]));
                }
                // Also show Ctrl keybindings
                self.push_line(Line::default());
                self.push_line(Line::from(Span::styled(
                    "Keybindings:",
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                let keys = [
                    ("Ctrl-T", "Cycle thinking level"),
                    ("Ctrl-C", "Abort / exit"),
                    ("Ctrl-D", "Exit"),
                    ("Tab", "Autocomplete slash commands"),
                    ("PgUp/PgDn", "Scroll output"),
                ];
                for (key, desc) in keys {
                    self.push_line(Line::from(vec![
                        Span::styled(format!("  {:<24}", key), Style::default().fg(Color::Cyan)),
                        Span::styled(desc.to_string(), Style::default().fg(Color::DarkGray)),
                    ]));
                }
                Some(None)
            }
            "/clear" => {
                self.output_lines.clear();
                self.scroll_offset = 0;
                self.auto_scroll = true;
                Some(None)
            }
            "/model" => {
                if args.is_empty() {
                    self.push_line(Line::from(Span::styled(
                        format!("Current model: {}", self.model_id),
                        Style::default().fg(Color::DarkGray),
                    )));
                    self.push_line(Line::from(Span::styled(
                        "Usage: /model <model-id>".to_string(),
                        Style::default().fg(Color::DarkGray),
                    )));
                } else {
                    ai::register_builtin_providers();
                    match ai::models::find_model(args) {
                        Some(model) => {
                            let new_id = model.id.clone();
                            let ctx = model.context_window;
                            agent.set_model((*model).clone());
                            self.model_id = new_id.clone();
                            self.context_window = ctx;
                            self.push_line(Line::from(Span::styled(
                                format!("[model: {}]", new_id),
                                Style::default().fg(Color::Magenta),
                            )));
                        }
                        None => {
                            self.push_line(Line::from(Span::styled(
                                format!("Unknown model '{}'", args),
                                Style::default().fg(Color::Red),
                            )));
                        }
                    }
                }
                Some(None)
            }
            "/thinking" => {
                if args.is_empty() {
                    let label = format!("{:?}", self.thinking_level).to_lowercase();
                    self.push_line(Line::from(Span::styled(
                        format!("Current thinking level: {}", label),
                        Style::default().fg(Color::DarkGray),
                    )));
                    self.push_line(Line::from(Span::styled(
                        "Usage: /thinking <off|low|medium|high>".to_string(),
                        Style::default().fg(Color::DarkGray),
                    )));
                } else {
                    let level: Result<ThinkingLevel, _> =
                        serde_json::from_value(serde_json::Value::String(args.to_string()));
                    match level {
                        Ok(l) => {
                            agent.set_thinking_level(l.clone());
                            self.thinking_level = l;
                            let label = format!("{:?}", self.thinking_level).to_lowercase();
                            self.push_line(Line::from(Span::styled(
                                format!("[thinking: {}]", label),
                                Style::default().fg(Color::Magenta),
                            )));
                        }
                        Err(_) => {
                            self.push_line(Line::from(Span::styled(
                                format!("Invalid thinking level '{}'. Use: off, low, medium, high, xhigh", args),
                                Style::default().fg(Color::Red),
                            )));
                        }
                    }
                }
                Some(None)
            }
            "/skills" => {
                if self.skills.is_empty() {
                    self.push_line(Line::from(Span::styled(
                        "No skills loaded.",
                        Style::default().fg(Color::DarkGray),
                    )));
                } else {
                    let skill_lines: Vec<Line<'static>> = self
                        .skills
                        .iter()
                        .map(|s| {
                            Line::from(vec![
                                Span::styled(
                                    format!("  /skill:{:<16}", s.name),
                                    Style::default().fg(Color::Cyan),
                                ),
                                Span::styled(
                                    s.description.clone(),
                                    Style::default().fg(Color::DarkGray),
                                ),
                            ])
                        })
                        .collect();
                    self.push_line(Line::from(Span::styled(
                        "Available skills:",
                        Style::default().add_modifier(Modifier::BOLD),
                    )));
                    for line in skill_lines {
                        self.push_line(line);
                    }
                }
                Some(None)
            }
            "/compact" => {
                let ctx_pct = if self.context_window > 0 {
                    (self.tokens_in + self.tokens_out) as f64 / self.context_window as f64 * 100.0
                } else {
                    0.0
                };
                self.push_line(Line::from(Span::styled(
                    format!(
                        "Tokens: {} in, {} out | Context: {:.1}% of {} | Cost: ${:.4}",
                        self.tokens_in,
                        self.tokens_out,
                        ctx_pct,
                        self.context_window,
                        self.total_cost
                    ),
                    Style::default().fg(Color::DarkGray),
                )));
                Some(None)
            }
            "/debug" => {
                self.debug = !self.debug;
                self.push_line(Line::from(Span::styled(
                    format!("[debug: {}]", if self.debug { "on" } else { "off" }),
                    Style::default().fg(Color::Magenta),
                )));
                Some(None)
            }
            _ if cmd.starts_with("/skill:") => {
                let skill_name = &cmd[7..];
                match skills::expand_skill_command(input, &self.skills) {
                    Some(expanded) => {
                        self.push_line(Line::from(Span::styled(
                            format!("[skill: {}]", skill_name),
                            Style::default().fg(Color::Blue),
                        )));
                        Some(Some(expanded))
                    }
                    None => {
                        self.push_line(Line::from(Span::styled(
                            format!("Unknown skill '{}'", skill_name),
                            Style::default().fg(Color::Red),
                        )));
                        Some(None)
                    }
                }
            }
            _ => {
                self.push_line(Line::from(Span::styled(
                    format!(
                        "Unknown command '{}'. Type /help for available commands.",
                        cmd
                    ),
                    Style::default().fg(Color::Red),
                )));
                Some(None)
            }
        }
    }

    /// Cycle thinking level: off → low → medium → high → off
    fn cycle_thinking(&mut self) -> ThinkingLevel {
        self.thinking_level = match self.thinking_level {
            ThinkingLevel::Off => ThinkingLevel::Low,
            ThinkingLevel::Minimal => ThinkingLevel::Low,
            ThinkingLevel::Low => ThinkingLevel::Medium,
            ThinkingLevel::Medium => ThinkingLevel::High,
            ThinkingLevel::High => ThinkingLevel::Off,
            ThinkingLevel::XHigh => ThinkingLevel::Off,
        };
        self.thinking_level.clone()
    }

    fn push_line(&mut self, line: Line<'static>) {
        self.output_lines.push(line);
        if self.auto_scroll {
            self.scroll_offset = 0;
        }
    }

    fn push_separator(&mut self) {
        self.push_line(Line::from(Span::styled(
            "─".repeat(60),
            Style::default().fg(Color::DarkGray),
        )));
    }

    /// Flush any partial streaming text into output_lines.
    fn flush_streaming(&mut self) {
        if self.streaming_text.is_empty() {
            return;
        }
        let style = if self.is_thinking {
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC)
        } else {
            Style::default()
        };
        for line_text in self.streaming_text.split('\n') {
            self.output_lines
                .push(Line::from(Span::styled(line_text.to_string(), style)));
        }
        self.streaming_text.clear();
        self.is_thinking = false;
        if self.auto_scroll {
            self.scroll_offset = 0;
        }
    }

    /// Process a text delta from the assistant.
    fn append_text_delta(&mut self, delta: &str) {
        self.streaming_text.push_str(delta);

        // Split completed lines into output_lines, keep partial last line
        while let Some(newline_pos) = self.streaming_text.find('\n') {
            let completed = self.streaming_text[..newline_pos].to_string();
            let style = if self.is_thinking {
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC)
            } else {
                Style::default()
            };
            self.output_lines
                .push(Line::from(Span::styled(completed, style)));
            self.streaming_text = self.streaming_text[newline_pos + 1..].to_string();
            if self.auto_scroll {
                self.scroll_offset = 0;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Event loop
// ---------------------------------------------------------------------------

async fn run_app(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<io::Stderr>>,
    agent: Arc<Agent>,
    config: TuiRunConfig,
) -> Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<AgentEvent>();
    let mut app = App::new(&config);

    // Initialize thinking level from agent state
    app.thinking_level = agent.with_state(|s| s.thinking_level.clone());

    // Set up TUI-aware permission prompt
    let (perm_req_tx, perm_req_rx) = std::sync::mpsc::channel::<(
        String,
        String,
        std::sync::mpsc::Sender<permissions::PromptResult>,
    )>();
    let perm_prompt_fn: permissions::PromptFn = Arc::new(move |tool_name: &str, desc: &str| {
        let (resp_tx, resp_rx) = std::sync::mpsc::channel();
        let _ = perm_req_tx.send((tool_name.to_string(), desc.to_string(), resp_tx));
        resp_rx.recv().unwrap_or(permissions::PromptResult::Deny)
    });
    config.permission_service.set_prompt_fn(perm_prompt_fn);

    // Permission state
    let mut pending_permission: Option<(
        String,
        std::sync::mpsc::Sender<permissions::PromptResult>,
    )> = None;

    // Subscribe to agent events → channel
    let tx_agent = tx.clone();
    let session_for_save = config.session_file.clone();
    let _unsub = agent.subscribe(move |event| {
        if let AgentEvent::MessageEnd { message } = &event {
            if let Some(ref sf) = session_for_save {
                let _ = sf.append(message);
            }
        }
        let _ = tx_agent.send(event.clone());
    });

    // Crossterm event stream
    let mut reader = EventStream::new();

    // Welcome line
    let welcome_model = app.model_id.clone();
    app.push_line(Line::from(vec![
        Span::styled(
            "τ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" — "),
        Span::styled(
            welcome_model,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    for msg in &config.startup_messages {
        app.push_line(Line::from(Span::styled(
            msg.clone(),
            Style::default().fg(Color::DarkGray),
        )));
    }
    app.push_separator();

    loop {
        terminal.draw(|f| ui(f, &app, &pending_permission))?;

        // Check for permission requests (non-blocking)
        // After abort, auto-deny any lingering permission requests
        if !app.is_busy {
            while let Ok((_desc, _text, resp_tx)) = perm_req_rx.try_recv() {
                let _ = resp_tx.send(permissions::PromptResult::Deny);
            }
        } else if pending_permission.is_none() {
            if let Ok((tool_desc, _desc_text, resp_tx)) = perm_req_rx.try_recv() {
                app.push_line(Line::from(vec![
                    Span::styled("[permission] ", Style::default().fg(Color::Magenta)),
                    Span::raw(tool_desc.clone()),
                    Span::styled(" — y/n/a: ", Style::default().fg(Color::Magenta)),
                ]));
                pending_permission = Some((tool_desc, resp_tx));
            }
        }

        tokio::select! {
            Some(Ok(term_event)) = reader.next() => {
                if let Some((ref _tool, ref resp_tx)) = pending_permission {
                    // In permission mode: accept y/n/a, or Ctrl-C to deny and abort
                    match term_event {
                        Event::Key(KeyEvent { code: KeyCode::Char('c'), modifiers: KeyModifiers::CONTROL, .. }) => {
                            // Ctrl-C: deny the permission and abort
                            let _ = resp_tx.send(permissions::PromptResult::Deny);
                            pending_permission = None;
                            agent.abort();
                            app.is_busy = false;
                            app.active_tools.clear();
                            app.flush_streaming();
                            app.push_line(Line::from(Span::styled(
                                "^C (aborted)",
                                Style::default().fg(Color::Yellow),
                            )));
                            app.push_separator();
                        }
                        Event::Key(KeyEvent { code: KeyCode::Char(c), modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT, .. }) => {
                            let result = match c {
                                'y' | 'Y' => Some(permissions::PromptResult::Allow),
                                'a' | 'A' => Some(permissions::PromptResult::AlwaysAllow),
                                'n' | 'N' => Some(permissions::PromptResult::Deny),
                                _ => None,
                            };
                            if let Some(r) = result {
                                let _ = resp_tx.send(r);
                                pending_permission = None;
                            }
                        }
                        _ => {}
                    }
                } else {
                    handle_terminal_event(&mut app, term_event, &agent, &tx);
                }
            }
            Some(agent_event) = rx.recv() => {
                // After abort, drain events silently until AgentEnd
                if !app.is_busy {
                    // Still process AgentEnd to clean up, ignore everything else
                    if matches!(agent_event, AgentEvent::AgentEnd { .. }) {
                        // Already handled by the abort
                    }
                } else {
                    handle_agent_event(&mut app, &agent_event);
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn ui(
    frame: &mut ratatui::Frame,
    app: &App,
    pending_permission: &Option<(String, std::sync::mpsc::Sender<permissions::PromptResult>)>,
) {
    let chunks = Layout::vertical([
        Constraint::Min(1),    // output area
        Constraint::Length(1), // input line
        Constraint::Length(1), // separator
        Constraint::Length(1), // status bar
    ])
    .split(frame.area());

    // Output area
    let output_height = chunks[0].height as usize;
    let _total_lines = app.output_lines.len();

    // Include current streaming text as a temporary line for display
    let mut display_lines = app.output_lines.clone();
    if !app.streaming_text.is_empty() {
        let style = if app.is_thinking {
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC)
        } else {
            Style::default()
        };
        display_lines.push(Line::from(Span::styled(app.streaming_text.clone(), style)));
    }

    // Bottom-align: pad with empty lines so content grows upward from the input line
    let total = display_lines.len();
    if total < output_height {
        let padding = output_height - total;
        let mut padded = vec![Line::default(); padding];
        padded.append(&mut display_lines);
        display_lines = padded;
    }

    // Calculate scroll
    let scroll = if app.auto_scroll {
        if display_lines.len() > output_height {
            (display_lines.len() - output_height) as u16
        } else {
            0
        }
    } else if display_lines.len() > output_height {
        let max_scroll = (display_lines.len() - output_height) as u16;
        max_scroll.saturating_sub(app.scroll_offset)
    } else {
        0
    };

    let output = Paragraph::new(display_lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(output, chunks[0]);

    // Input line
    let prompt_text = if pending_permission.is_some() {
        "[y/n/a]> ".to_string()
    } else if app.is_busy {
        format!("{}  ", app.model_id)
    } else {
        format!("{}> ", app.model_id)
    };
    let input_line = Line::from(vec![
        Span::styled(&prompt_text, Style::default().fg(Color::Cyan)),
        Span::raw(&app.input),
    ]);
    frame.render_widget(Paragraph::new(input_line), chunks[1]);

    // Set cursor
    if !app.is_busy && pending_permission.is_none() {
        let cursor_x = chunks[1].x + prompt_text.len() as u16 + app.cursor_pos as u16;
        let cursor_y = chunks[1].y;
        frame.set_cursor_position((cursor_x, cursor_y));
    }

    // Separator
    let sep = Line::from("─".repeat(chunks[2].width as usize));
    frame.render_widget(
        Paragraph::new(sep).style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );

    // Status bar
    let status = build_status_line(app);
    frame.render_widget(
        Paragraph::new(status).style(Style::default().fg(Color::DarkGray)),
        chunks[3],
    );
}

fn build_status_line(app: &App) -> Line<'static> {
    let ctx_pct = if app.context_window > 0 {
        ((app.tokens_in + app.tokens_out) as f64 / app.context_window as f64 * 100.0) as u64
    } else {
        0
    };

    let mut spans = vec![
        Span::styled(
            format!(" {} ", app.model_id),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("| "),
        Span::raw(format!(
            "{}in {}out ",
            fmt_tokens(app.tokens_in),
            fmt_tokens(app.tokens_out)
        )),
        Span::raw("| "),
        Span::raw(format!("ctx {}% ", ctx_pct)),
        Span::raw("| "),
        Span::raw(format!("${:.3} ", app.total_cost)),
    ];

    // Thinking level (only show if not off)
    if app.thinking_level != ThinkingLevel::Off {
        spans.push(Span::raw("| "));
        let label = format!("{:?}", app.thinking_level).to_lowercase();
        spans.push(Span::styled(
            format!("think:{} ", label),
            Style::default().fg(Color::Magenta),
        ));
    }

    if !app.active_tools.is_empty() {
        spans.push(Span::raw("| "));
        let tool_text = if app.active_tools.len() == 1 {
            app.active_tools[0].clone()
        } else {
            format!("{} +{}", app.active_tools[0], app.active_tools.len() - 1)
        };
        spans.push(Span::styled(tool_text, Style::default().fg(Color::Yellow)));
        spans.push(Span::raw(" "));
    }

    Line::from(spans)
}

fn fmt_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M ", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K ", n as f64 / 1_000.0)
    } else {
        format!("{} ", n)
    }
}

// ---------------------------------------------------------------------------
// Terminal event handling (keyboard input)
// ---------------------------------------------------------------------------

fn handle_terminal_event(
    app: &mut App,
    event: Event,
    agent: &Arc<Agent>,
    tx: &tokio::sync::mpsc::UnboundedSender<AgentEvent>,
) {
    match event {
        Event::Key(KeyEvent {
            code, modifiers, ..
        }) => {
            app.debug_log(format!("key: {:?} modifiers: {:?}", code, modifiers));
            match (code, modifiers) {
                // Ctrl-C
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                    if app.is_busy {
                        agent.abort();
                        app.is_busy = false;
                        app.active_tools.clear();
                        app.flush_streaming();
                        app.push_line(Line::from(Span::styled(
                            "^C (aborted)",
                            Style::default().fg(Color::Yellow),
                        )));
                        app.push_separator();
                    } else {
                        let count = app.abort_count.fetch_add(1, Ordering::SeqCst);
                        if count >= 1 {
                            app.should_quit = true;
                        } else {
                            app.push_line(Line::from(Span::styled(
                                "^C (press again to exit)",
                                Style::default().fg(Color::DarkGray),
                            )));
                        }
                    }
                }
                // Ctrl-D
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    app.should_quit = true;
                }
                // Ctrl-T: cycle thinking level
                (KeyCode::Char('t'), KeyModifiers::CONTROL) => {
                    let new_level = app.cycle_thinking();
                    agent.set_thinking_level(new_level.clone());
                    let label = format!("{:?}", new_level).to_lowercase();
                    app.push_line(Line::from(Span::styled(
                        format!("[thinking: {}]", label),
                        Style::default().fg(Color::Magenta),
                    )));
                }
                // Enter
                (KeyCode::Enter, _) => {
                    if !app.is_busy && !app.input.trim().is_empty() {
                        app.reset_tab();
                        let input = app.input.trim().to_string();
                        app.input.clear();
                        app.cursor_pos = 0;
                        app.abort_count.store(0, Ordering::SeqCst);

                        // Try slash command dispatch first
                        if let Some(result) = app.handle_slash_command(&input, agent) {
                            match result {
                                None => return, // Handled locally
                                Some(expanded) => {
                                    // Skill expansion — echo and send to LLM
                                    app.push_line(Line::from(Span::styled(
                                        format!("{}> {}", app.model_id, input),
                                        Style::default()
                                            .fg(Color::Cyan)
                                            .add_modifier(Modifier::BOLD),
                                    )));
                                    app.is_busy = true;
                                    let agent = Arc::clone(agent);
                                    let tx = tx.clone();
                                    tokio::spawn(async move {
                                        let result = agent.prompt(expanded).await;
                                        if let Err(e) = result {
                                            let _ = tx.send(AgentEvent::AgentEnd {
                                                messages: vec![AgentMessage::user(format!(
                                                    "Error: {}",
                                                    e
                                                ))],
                                            });
                                        }
                                    });
                                    return;
                                }
                            }
                        }

                        // Regular prompt — echo and send to LLM
                        app.push_line(Line::from(Span::styled(
                            format!("{}> {}", app.model_id, input),
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        )));

                        app.is_busy = true;
                        let agent = Arc::clone(agent);
                        let tx = tx.clone();
                        tokio::spawn(async move {
                            let result = agent.prompt(input).await;
                            if let Err(e) = result {
                                // Send a synthetic error event
                                let _ = tx.send(AgentEvent::AgentEnd {
                                    messages: vec![AgentMessage::user(format!("Error: {}", e))],
                                });
                            }
                        });
                    }
                }
                // Ctrl-W: delete word backward
                (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
                    if app.cursor_pos > 0 && !app.is_busy {
                        app.reset_tab();
                        let mut pos = app.cursor_pos;
                        // Skip trailing whitespace
                        while pos > 0 && app.input.as_bytes()[pos - 1] == b' ' {
                            pos -= 1;
                        }
                        // Delete back to previous whitespace or start
                        while pos > 0 && app.input.as_bytes()[pos - 1] != b' ' {
                            pos -= 1;
                        }
                        app.input.drain(pos..app.cursor_pos);
                        app.cursor_pos = pos;
                    }
                }
                // Backspace
                (KeyCode::Backspace, _) => {
                    if app.cursor_pos > 0 && !app.is_busy {
                        app.reset_tab();
                        app.input.remove(app.cursor_pos - 1);
                        app.cursor_pos -= 1;
                    }
                }
                // Delete
                (KeyCode::Delete, _) => {
                    if app.cursor_pos < app.input.len() && !app.is_busy {
                        app.reset_tab();
                        app.input.remove(app.cursor_pos);
                    }
                }
                // Left arrow
                (KeyCode::Left, _) => {
                    if app.cursor_pos > 0 {
                        app.cursor_pos -= 1;
                    }
                }
                // Right arrow
                (KeyCode::Right, _) => {
                    if app.cursor_pos < app.input.len() {
                        app.cursor_pos += 1;
                    }
                }
                // Home
                (KeyCode::Home, _) => {
                    app.cursor_pos = 0;
                }
                // End
                (KeyCode::End, _) => {
                    app.cursor_pos = app.input.len();
                }
                // Tab: slash command completion
                (KeyCode::Tab, _) => {
                    if !app.is_busy {
                        app.tab_complete();
                    }
                }
                // PageUp
                (KeyCode::PageUp, _) => {
                    app.auto_scroll = false;
                    app.scroll_offset = app.scroll_offset.saturating_add(10);
                }
                // PageDown
                (KeyCode::PageDown, _) => {
                    if app.scroll_offset <= 10 {
                        app.scroll_offset = 0;
                        app.auto_scroll = true;
                    } else {
                        app.scroll_offset = app.scroll_offset.saturating_sub(10);
                    }
                }
                // Regular character
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    if !app.is_busy {
                        app.reset_tab();
                        app.input.insert(app.cursor_pos, c);
                        app.cursor_pos += 1;
                    }
                }
                _ => {}
            }
        }
        Event::Resize(_, _) => {
            // ratatui handles resize automatically on next draw
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Agent event handling
// ---------------------------------------------------------------------------

fn handle_agent_event(app: &mut App, event: &AgentEvent) {
    match event {
        AgentEvent::MessageUpdate {
            assistant_event, ..
        } => match assistant_event.as_ref() {
            AssistantMessageEvent::TextDelta { delta, .. } => {
                app.is_thinking = false;
                app.append_text_delta(delta);
            }
            AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                app.is_thinking = true;
                app.append_text_delta(delta);
            }
            _ => {}
        },
        AgentEvent::ToolExecutionStart {
            tool_name, args, ..
        } => {
            app.active_tools.push(tool_name.clone());

            let detail = extract_tool_detail(tool_name, args);
            let mut spans = vec![
                Span::styled("[tool: ", Style::default().fg(Color::Blue)),
                Span::styled(
                    tool_name.to_string(),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("] ", Style::default().fg(Color::Blue)),
            ];
            if let Some(d) = detail {
                spans.push(Span::styled(d, Style::default().fg(Color::DarkGray)));
            }
            app.push_line(Line::from(spans));
        }
        AgentEvent::ToolExecutionEnd {
            tool_name,
            is_error,
            ..
        } => {
            app.active_tools.retain(|t| t != tool_name);
            if *is_error {
                app.push_line(Line::from(Span::styled(
                    format!("[tool error: {}]", tool_name),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )));
            }
        }
        AgentEvent::TurnEnd {
            message: AgentMessage::Llm(Message::Assistant(am)),
            ..
        } => {
            app.tokens_in += am.usage.input;
            app.tokens_out += am.usage.output;
            app.total_cost += am.usage.cost.total;
        }
        AgentEvent::AgentEnd { .. } => {
            app.flush_streaming();
            app.is_busy = false;
            app.push_separator();
        }
        _ => {}
    }
}

fn extract_tool_detail(tool_name: &str, args: &serde_json::Value) -> Option<String> {
    match tool_name {
        "file_read" | "file_write" | "file_edit" => {
            args.get("path").and_then(|v| v.as_str()).map(String::from)
        }
        "glob" | "grep" => args
            .get("pattern")
            .and_then(|v| v.as_str())
            .map(String::from),
        "bash" => args.get("command").and_then(|v| v.as_str()).map(|s| {
            let line = s.lines().next().unwrap_or(s);
            if line.len() > 80 {
                format!("{}...", &line[..77])
            } else if s.lines().count() > 1 {
                format!("{}...", line)
            } else {
                line.to_string()
            }
        }),
        "web_fetch" => args.get("url").and_then(|v| v.as_str()).map(String::from),
        "web_search" => args.get("query").and_then(|v| v.as_str()).map(String::from),
        "subagent" => args.get("task").and_then(|v| v.as_str()).map(|s| {
            let line = s.lines().next().unwrap_or(s);
            if line.len() > 60 {
                format!("{}...", &line[..57])
            } else {
                line.to_string()
            }
        }),
        "todo" => args.get("todos").and_then(|v| v.as_array()).map(|todos| {
            let total = todos.len();
            let done = todos
                .iter()
                .filter(|t| t.get("status").and_then(|s| s.as_str()) == Some("completed"))
                .count();
            format!("[{}/{}]", done, total)
        }),
        _ => None,
    }
}
