use std::sync::Arc;

use agent::types::AgentTool;

use crate::skills::Skill;

// Static prompt sections — embedded at compile time from markdown files.
const IDENTITY: &str = include_str!("../prompts/identity.md");
const SYSTEM: &str = include_str!("../prompts/system.md");
const DOING_TASKS: &str = include_str!("../prompts/doing_tasks.md");
const EXECUTING_WITH_CARE: &str = include_str!("../prompts/executing_with_care.md");
const TONE_AND_OUTPUT: &str = include_str!("../prompts/tone_and_output.md");
// Orchestration prompt — split across multiple files for pattern-level iteration.
const ORCH_OVERVIEW: &str = include_str!("../prompts/orchestration/overview.md");
const ORCH_TOOLS: &str = include_str!("../prompts/orchestration/tools.md");
const ORCH_DOCUMENTS: &str = include_str!("../prompts/orchestration/documents.md");
const ORCH_FANOUT: &str = include_str!("../prompts/orchestration/patterns/fanout.md");
const ORCH_PIPELINE: &str = include_str!("../prompts/orchestration/patterns/pipeline.md");
const ORCH_ADVERSARIAL: &str = include_str!("../prompts/orchestration/patterns/adversarial.md");
const ORCH_REUSE: &str = include_str!("../prompts/orchestration/patterns/reuse.md");
const ORCH_PROGRAMMATIC: &str = include_str!("../prompts/orchestration/patterns/programmatic.md");
const PY_TAU_API_REFERENCE: &str = include_str!("../prompts/generated/py_tau_api_reference.md");
const PY_REPL_GUIDELINE: &str = include_str!("../prompts/generated/py_repl_guideline.txt");
const ORCH_REACTIVE: &str = include_str!("../prompts/orchestration/patterns/reactive.md");
const ORCH_WF_FEATURE: &str = include_str!("../prompts/orchestration/workflows/feature.md");
const ORCH_WF_BUGFIX: &str = include_str!("../prompts/orchestration/workflows/bugfix.md");
const ORCH_WF_REFACTOR: &str = include_str!("../prompts/orchestration/workflows/refactor.md");
const ORCH_WF_RESEARCH: &str = include_str!("../prompts/orchestration/workflows/research.md");
const ORCH_WF_SUPERVISED: &str = include_str!("../prompts/orchestration/workflows/supervised.md");
const ORCH_WF_SESSION_INIT: &str =
    include_str!("../prompts/orchestration/workflows/session_init.md");

/// Truncate a description to the first sentence (up to the first '.').
fn first_sentence(desc: &str) -> &str {
    if let Some(pos) = desc.find('.') {
        &desc[..=pos]
    } else {
        desc
    }
}

/// Build a dynamic system prompt from the registered tools, skills, and current working directory.
pub fn build_system_prompt(tools: &[Arc<dyn AgentTool>], skills: &[Skill], cwd: &str) -> String {
    let mut parts: Vec<String> = Vec::new();

    let tool_names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    let has = |name: &str| tool_names.contains(&name);

    // ── 1–4. Static sections ──
    parts.push(IDENTITY.to_string());
    parts.push(SYSTEM.to_string());
    parts.push(DOING_TASKS.to_string());
    parts.push(EXECUTING_WITH_CARE.to_string());

    // ── 5. Available tools (dynamic) ──
    if tools.is_empty() {
        parts.push("# Available tools\n(none)".to_string());
    } else {
        let mut listing = "# Available tools".to_string();
        for tool in tools {
            let one_liner = first_sentence(tool.description());
            listing.push_str(&format!("\n- {}: {}", tool.name(), one_liner));
        }
        parts.push(listing);
    }

    // ── 6. Using your tools (conditional on available tools) ──
    let mut guidelines: Vec<String> = Vec::new();

    // Core principle: prefer dedicated tools over bash
    if has("bash") {
        let mut bash_msg = "Do NOT use bash when a dedicated tool is provided:".to_string();
        if has("file_read") {
            bash_msg
                .push_str("\n  - To read files use file_read instead of cat, head, tail, or sed.");
        }
        if has("file_edit") {
            bash_msg.push_str("\n  - To edit files use file_edit instead of sed or awk.");
        }
        if has("file_write") {
            bash_msg.push_str(
                "\n  - To create new files use file_write instead of heredoc or echo redirection.",
            );
        }
        if has("grep") {
            bash_msg.push_str("\n  - To search file contents use grep instead of bash grep/rg.");
        }
        if has("glob") {
            bash_msg.push_str("\n  - To find files by pattern use glob instead of bash find/ls.");
        }
        bash_msg.push_str(
            "\n  - Reserve bash for system commands and operations that require shell execution.",
        );
        guidelines.push(bash_msg);
    }

    if has("file_read") && has("file_edit") {
        guidelines.push(
            "Read files before editing them. Follow the file_edit tool's description \
             precisely for the edit format."
                .to_string(),
        );
    }
    if has("file_write") {
        guidelines.push(
            "Use file_write only for new files or complete rewrites, not for surgical edits."
                .to_string(),
        );
    }
    if has("grep") {
        guidelines.push(
            "Use grep for searching file contents — prefer it over bash grep/rg commands."
                .to_string(),
        );
    }
    if has("bash") && !has("grep") && !has("find") && !has("glob") {
        guidelines.push("Use bash for file exploration (ls, grep, find).".to_string());
    }
    if has("glob") {
        guidelines.push(
            "Use glob for finding files by pattern — prefer it over bash find/ls commands."
                .to_string(),
        );
    }
    if has("subagent") {
        guidelines.push(
            "Use subagent to delegate well-defined subtasks — especially exploratory \
             research, file analysis, or work that would clutter your context. Give each \
             sub-agent a clear, self-contained task description. Consider using a cheaper \
             model (via the model parameter) for straightforward work."
                .to_string(),
        );
    }
    if has("thread") || has("query") {
        guidelines.push(
            "Use thread and query for orchestration. See the dedicated orchestration section \
             below for patterns and guidance. Prefer thread over subagent."
                .to_string(),
        );
    }
    if has("py_repl") {
        guidelines.push(PY_REPL_GUIDELINE.trim().to_string());
    }
    if has("todo") {
        guidelines.push(
            "Use todo to track progress on tasks with 3+ steps. Send the full todo list \
             each call — it replaces the previous list. Keep exactly one task \"in_progress\" \
             at a time. Mark tasks \"completed\" immediately after finishing, then drop them \
             on the next update."
                .to_string(),
        );
    }

    // Parallel tool calls
    guidelines.push(
        "You can call multiple tools in a single response. If there are no dependencies \
         between calls, make all independent tool calls in parallel for efficiency."
            .to_string(),
    );

    if !guidelines.is_empty() {
        let mut section = "# Using your tools".to_string();
        for g in &guidelines {
            section.push_str(&format!("\n- {}", g));
        }
        parts.push(section);
    }

    // ── 7. Core orchestration (conditional on direct orchestration tools) ──
    if has("thread") || has("query") {
        parts.push(
            [
                ORCH_OVERVIEW,
                ORCH_TOOLS,
                ORCH_DOCUMENTS,
                ORCH_FANOUT,
                ORCH_PIPELINE,
                ORCH_ADVERSARIAL,
                ORCH_REUSE,
            ]
            .join("\n\n"),
        );
    }

    // ── 8. py_repl addendum (conditional only on py_repl) ──
    if has("py_repl") {
        parts.push(
            [
                ORCH_PROGRAMMATIC,
                PY_TAU_API_REFERENCE,
                ORCH_REACTIVE,
                ORCH_WF_FEATURE,
                ORCH_WF_BUGFIX,
                ORCH_WF_REFACTOR,
                ORCH_WF_RESEARCH,
                ORCH_WF_SUPERVISED,
                ORCH_WF_SESSION_INIT,
            ]
            .join("\n\n"),
        );
    }

    // ── 9. Tone and output (static) ──
    parts.push(TONE_AND_OUTPUT.to_string());

    // ── 10. Skills (dynamic — progressive disclosure) ──
    if !skills.is_empty() && has("file_read") {
        let mut section = "# Available skills\n\
             Skills are invoked with `/skill:<name> [args]`. Do NOT load skills automatically; \
             only use them when the user explicitly invokes a slash command:"
            .to_string();
        for skill in skills {
            section.push_str(&format!(
                "\n- {}: {}\n  Path: {}",
                skill.name,
                skill.description,
                skill.file_path.display()
            ));
        }
        parts.push(section);
    }

    // ── 11. Environment (dynamic) ──
    parts.push(format!("# Environment\nCurrent working directory: {}", cwd));

    parts.join("\n\n")
}
