use coding_agent::config::EditMode;
use coding_agent::system_prompt::build_system_prompt;
use coding_agent::tools::{
    BashTool, FileEditTool, FileReadTool, FileWriteTool, GlobTool, GrepTool,
};
use std::sync::Arc;

fn all_tools() -> Vec<Arc<dyn agent::types::AgentTool>> {
    vec![
        BashTool::arc(),
        FileReadTool::arc(EditMode::Replace),
        FileEditTool::arc(EditMode::Replace),
        FileWriteTool::arc(),
        GlobTool::arc(),
        GrepTool::arc(),
    ]
}

// INV-1: The prompt always contains every registered tool's name.
#[test]
fn system_prompt_contains_all_tool_names() {
    let tools = all_tools();
    let prompt = build_system_prompt(&tools, &[], "/tmp");
    for tool in &tools {
        assert!(
            prompt.contains(tool.name()),
            "prompt should contain tool name '{}', got:\n{}",
            tool.name(),
            prompt
        );
    }
}

// INV-2: The prompt always contains "current working directory" with the cwd value.
#[test]
fn system_prompt_contains_cwd_in_footer() {
    let tools = all_tools();
    let cwd = "/home/user/projects/myapp";
    let prompt = build_system_prompt(&tools, &[], cwd);
    assert!(
        prompt.to_lowercase().contains("current working directory"),
        "prompt should contain 'current working directory'"
    );
    assert!(
        prompt.contains(cwd),
        "prompt should contain cwd '{}', got:\n{}",
        cwd,
        prompt
    );
}

// INV-3: When file_read and file_edit are both present, "read before editing" guideline appears.
#[test]
fn system_prompt_read_before_edit_guideline_when_both_present() {
    let tools: Vec<Arc<dyn agent::types::AgentTool>> = vec![
        FileReadTool::arc(EditMode::Replace),
        FileEditTool::arc(EditMode::Replace),
    ];
    let prompt = build_system_prompt(&tools, &[], "/tmp");
    assert!(
        prompt.to_lowercase().contains("read files before editing"),
        "prompt should contain read-before-edit guideline, got:\n{}",
        prompt
    );
}

// INV-4: When only bash is present (no grep/glob/find tools), the "use bash for file exploration"
// guideline appears.
#[test]
fn system_prompt_bash_exploration_guideline_when_only_bash() {
    let tools: Vec<Arc<dyn agent::types::AgentTool>> = vec![BashTool::arc()];
    let prompt = build_system_prompt(&tools, &[], "/tmp");
    assert!(
        prompt
            .to_lowercase()
            .contains("use bash for file exploration"),
        "prompt should contain bash-for-exploration guideline, got:\n{}",
        prompt
    );
}

// Default tool set produces a sensible prompt with all expected sections.
#[test]
fn system_prompt_default_tool_set_is_sensible() {
    let tools = all_tools();
    let prompt = build_system_prompt(&tools, &[], "/workspace");

    assert!(prompt.contains("expert coding assistant"));
    assert!(prompt.contains("# Available tools"));
    assert!(prompt.contains("# Using your tools"));
    assert!(prompt.contains("# Doing tasks"));
    assert!(prompt.contains("# Executing actions with care"));
    assert!(prompt.contains("# Tone and output"));
    assert!(prompt.contains("Current working directory: /workspace"));
}

// Empty tool set produces a prompt with "(none)", does not crash.
#[test]
fn system_prompt_empty_tools_does_not_crash() {
    let tools: Vec<Arc<dyn agent::types::AgentTool>> = vec![];
    let prompt = build_system_prompt(&tools, &[], "/some/dir");

    assert!(
        prompt.contains("(none)"),
        "empty tool set should show '(none)', got:\n{}",
        prompt
    );
    assert!(prompt.contains("Current working directory: /some/dir"));
}

// INV-3 negative: Without file_read, the "read before editing" guideline is absent.
#[test]
fn system_prompt_no_read_before_edit_without_file_read() {
    let tools: Vec<Arc<dyn agent::types::AgentTool>> = vec![FileEditTool::arc(EditMode::Replace)];
    let prompt = build_system_prompt(&tools, &[], "/tmp");
    assert!(
        !prompt.to_lowercase().contains("read files before editing"),
        "should NOT contain read-before-edit guideline without file_read, got:\n{}",
        prompt
    );
}

// Hashline mode tools still report as file_read/file_edit and include the
// "read before editing" guideline via the unified name.
#[test]
fn system_prompt_hashline_tools_use_canonical_names() {
    let tools: Vec<Arc<dyn agent::types::AgentTool>> = vec![
        BashTool::arc(),
        FileReadTool::arc(EditMode::Hashline),
        FileEditTool::arc(EditMode::Hashline),
        FileWriteTool::arc(),
    ];
    let prompt = build_system_prompt(&tools, &[], "/tmp");
    assert!(
        prompt.contains("file_read"),
        "prompt should mention file_read, got:\n{}",
        prompt
    );
    assert!(
        prompt.contains("file_edit"),
        "prompt should mention file_edit, got:\n{}",
        prompt
    );
    // Should NOT contain the old hash_file_* names
    assert!(
        !prompt.contains("hash_file_read"),
        "prompt should NOT mention hash_file_read, got:\n{}",
        prompt
    );
    assert!(
        !prompt.contains("hash_file_edit"),
        "prompt should NOT mention hash_file_edit, got:\n{}",
        prompt
    );
    // The "read before editing" guideline should still appear (same tool names)
    assert!(
        prompt.to_lowercase().contains("read files before editing"),
        "prompt should contain read-before-edit guideline for hashline tools, got:\n{}",
        prompt
    );
}

// When glob is present, the glob guideline appears.
#[test]
fn system_prompt_glob_guideline_when_glob_present() {
    let tools: Vec<Arc<dyn agent::types::AgentTool>> = vec![BashTool::arc(), GlobTool::arc()];
    let prompt = build_system_prompt(&tools, &[], "/tmp");
    assert!(
        prompt.contains("glob for finding files by pattern"),
        "prompt should contain glob guideline, got:\n{}",
        prompt
    );
}

// Standard tools → no hashline-specific content in system prompt body.
#[test]
fn system_prompt_no_hashline_guidelines_with_standard_tools() {
    let tools = all_tools();
    let prompt = build_system_prompt(&tools, &[], "/tmp");
    assert!(
        !prompt.contains("LINE#HASH"),
        "standard tools should not have hashline guidelines in system prompt, got:\n{}",
        prompt
    );
}

// INV-5: When grep is present, grep guideline appears.
#[test]
fn system_prompt_grep_guideline_when_grep_present() {
    let tools: Vec<Arc<dyn agent::types::AgentTool>> = vec![BashTool::arc(), GrepTool::arc()];
    let prompt = build_system_prompt(&tools, &[], "/tmp");
    assert!(
        prompt.to_lowercase().contains("use grep for searching"),
        "prompt should contain grep guideline, got:\n{}",
        prompt
    );
}

// INV-5 negative: When grep is present, "use bash for file exploration" guideline is absent.
#[test]
fn system_prompt_no_bash_exploration_when_grep_present() {
    let tools: Vec<Arc<dyn agent::types::AgentTool>> = vec![BashTool::arc(), GrepTool::arc()];
    let prompt = build_system_prompt(&tools, &[], "/tmp");
    assert!(
        !prompt
            .to_lowercase()
            .contains("use bash for file exploration"),
        "should NOT contain bash-for-exploration guideline when grep is present, got:\n{}",
        prompt
    );
}

// INV-6: When glob is present, "use bash for file exploration" guideline is absent.
#[test]
fn system_prompt_no_bash_exploration_when_glob_present() {
    let tools: Vec<Arc<dyn agent::types::AgentTool>> = vec![BashTool::arc(), GlobTool::arc()];
    let prompt = build_system_prompt(&tools, &[], "/tmp");
    assert!(
        !prompt
            .to_lowercase()
            .contains("use bash for file exploration"),
        "should NOT contain bash-for-exploration guideline when glob is present, got:\n{}",
        prompt
    );
}

// The "prefer dedicated tools over bash" section appears when bash + other tools coexist.
#[test]
fn system_prompt_prefer_dedicated_tools_over_bash() {
    let tools = all_tools();
    let prompt = build_system_prompt(&tools, &[], "/tmp");
    assert!(
        prompt
            .to_lowercase()
            .contains("do not use bash when a dedicated tool"),
        "prompt should instruct preferring dedicated tools over bash, got:\n{}",
        prompt
    );
}
