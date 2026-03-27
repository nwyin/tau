use std::sync::Arc;

use agent::types::AgentTool;

use crate::skills::Skill;

// Static prompt sections — embedded at compile time from markdown files.
const IDENTITY: &str = include_str!("../prompts/identity.md");
const SYSTEM: &str = include_str!("../prompts/system.md");
const DOING_TASKS: &str = include_str!("../prompts/doing_tasks.md");
const EXECUTING_WITH_CARE: &str = include_str!("../prompts/executing_with_care.md");
const TONE_AND_OUTPUT: &str = include_str!("../prompts/tone_and_output.md");

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

    // ── 7. Tone and output (static) ──
    parts.push(TONE_AND_OUTPUT.to_string());

    // ── 8. Skills (dynamic — progressive disclosure) ──
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

    // ── 9. Environment (dynamic) ──
    parts.push(format!("# Environment\nCurrent working directory: {}", cwd));

    parts.join("\n\n")
}
