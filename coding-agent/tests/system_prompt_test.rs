use coding_agent::system_prompt::build_system_prompt;
use coding_agent::tools::{BashTool, FileEditTool, FileReadTool, FileWriteTool};
use std::sync::Arc;

fn all_tools() -> Vec<Arc<dyn agent::types::AgentTool>> {
    vec![BashTool::arc(), FileReadTool::arc(), FileEditTool::arc(), FileWriteTool::arc()]
}

// INV-1: The prompt always contains every registered tool's name.
#[test]
fn system_prompt_contains_all_tool_names() {
    let tools = all_tools();
    let prompt = build_system_prompt(&tools, "/tmp");
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
    let prompt = build_system_prompt(&tools, cwd);
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
    let tools: Vec<Arc<dyn agent::types::AgentTool>> =
        vec![FileReadTool::arc(), FileEditTool::arc()];
    let prompt = build_system_prompt(&tools, "/tmp");
    assert!(
        prompt.to_lowercase().contains("read files before editing"),
        "prompt should contain read-before-edit guideline, got:\n{}",
        prompt
    );
}

// INV-4: When only bash is present (no grep/find tools), the "use bash for file exploration"
// guideline appears.
#[test]
fn system_prompt_bash_exploration_guideline_when_only_bash() {
    let tools: Vec<Arc<dyn agent::types::AgentTool>> = vec![BashTool::arc()];
    let prompt = build_system_prompt(&tools, "/tmp");
    assert!(
        prompt.to_lowercase().contains("use bash for file exploration"),
        "prompt should contain bash-for-exploration guideline, got:\n{}",
        prompt
    );
}

// Default tool set produces a sensible prompt with all expected sections.
#[test]
fn system_prompt_default_tool_set_is_sensible() {
    let tools = all_tools();
    let prompt = build_system_prompt(&tools, "/workspace");

    assert!(prompt.contains("expert coding assistant"));
    assert!(prompt.contains("Available tools:"));
    assert!(prompt.contains("Guidelines:"));
    assert!(prompt.contains("Current working directory: /workspace"));
}

// Empty tool set produces a prompt with "(none)", does not crash.
#[test]
fn system_prompt_empty_tools_does_not_crash() {
    let tools: Vec<Arc<dyn agent::types::AgentTool>> = vec![];
    let prompt = build_system_prompt(&tools, "/some/dir");

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
    let tools: Vec<Arc<dyn agent::types::AgentTool>> = vec![FileEditTool::arc()];
    let prompt = build_system_prompt(&tools, "/tmp");
    assert!(
        !prompt.to_lowercase().contains("read files before editing"),
        "should NOT contain read-before-edit guideline without file_read, got:\n{}",
        prompt
    );
}
