use std::sync::Arc;

use agent::types::AgentTool;

use crate::skills::Skill;

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

    // 1. Identity
    parts.push(
        "You are an expert coding assistant. You help users by reading files, executing commands, editing code, and writing new files.".to_string(),
    );

    // 2. Tool listing
    if tools.is_empty() {
        parts.push("Available tools:\n(none)".to_string());
    } else {
        let mut listing = "Available tools:".to_string();
        for tool in tools {
            let one_liner = first_sentence(tool.description());
            listing.push_str(&format!("\n- {}: {}", tool.name(), one_liner));
        }
        parts.push(listing);
    }

    // 3. Conditional guidelines
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    let has = |name: &str| tool_names.contains(&name);

    let mut guidelines: Vec<&str> = Vec::new();

    if has("file_read") && has("file_edit") {
        guidelines.push("Read files before editing them. Use file_read instead of cat or sed.");
        guidelines.push("Follow the file_edit tool's description precisely for the edit format.");
    }
    if has("file_write") {
        guidelines.push(
            "Use file_write only for new files or complete rewrites, not for surgical edits.",
        );
    }
    if has("grep") {
        guidelines
            .push("Use grep for searching file contents — prefer it over bash grep/rg commands.");
    }
    if has("bash") && !has("grep") && !has("find") {
        guidelines.push("Use bash for file exploration (ls, grep, find).");
    }
    if has("glob") {
        guidelines
            .push("Use glob for finding files by pattern — prefer it over bash find/ls commands.");
    }

    if has("subagent") {
        guidelines.push("Use subagent to delegate well-defined subtasks — especially exploratory research, file analysis, or work that would clutter your context. Give each sub-agent a clear, self-contained task description. Consider using a cheaper model (via the model parameter) for straightforward work.");
    }

    // Always-on guidelines
    guidelines.push("Be concise in your responses.");
    guidelines.push("Show file paths clearly when working with files.");
    guidelines.push("Work in the current working directory unless explicitly told otherwise.");

    if !guidelines.is_empty() {
        let mut section = "Guidelines:".to_string();
        for g in &guidelines {
            section.push_str(&format!("\n- {}", g));
        }
        parts.push(section);
    }

    // 4. Skills (progressive disclosure — only name + description + path)
    if !skills.is_empty() && has("file_read") {
        let mut section =
            "Available skills (use file_read to load the full skill when the task matches its description):"
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

    // 5. Footer
    parts.push(format!("Current working directory: {}", cwd));

    parts.join("\n\n")
}
