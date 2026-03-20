pub mod cli;
pub mod config;
pub mod session;
pub mod system_prompt;
pub mod tools;
pub mod trace;

pub use tools::all_tools;

/// Validate benchmark mode constraints.
///
/// Call this after building the tool list and resolving the test command, but before
/// constructing the agent. Returns an error if any invariant is violated.
///
/// Parameters:
/// - `has_tools_flag`: true iff `--tools` was explicitly provided
/// - `has_trace_output`: true iff `--trace-output` was explicitly provided
/// - `tool_names`: names of all resolved tools (after allowlist filtering)
/// - `test_cmd`: resolved test command (CLI flag or env var), if any
pub fn validate_benchmark_mode(
    has_tools_flag: bool,
    has_trace_output: bool,
    tool_names: &[&str],
    test_cmd: Option<&str>,
) -> anyhow::Result<()> {
    if !has_tools_flag {
        anyhow::bail!("benchmark mode requires --tools (explicit tool selection)");
    }
    if !has_trace_output {
        anyhow::bail!("benchmark mode requires --trace-output <DIR>");
    }
    if tool_names.contains(&"bash") {
        anyhow::bail!("benchmark mode does not allow bash tool — remove 'bash' from --tools");
    }
    if test_cmd.is_none() {
        anyhow::bail!("benchmark mode requires --test-command or TAU_BENCHMARK_TEST_CMD");
    }
    Ok(())
}

/// Resolve the prompt text from the CLI argument.
/// If `prompt_arg` is "-", reads all of `reader` (stdin in production).
pub fn resolve_prompt_text_from(
    prompt_arg: &str,
    reader: &mut dyn std::io::Read,
) -> anyhow::Result<String> {
    if prompt_arg == "-" {
        let mut text = String::new();
        reader.read_to_string(&mut text)?;
        Ok(text.trim().to_string())
    } else {
        Ok(prompt_arg.to_string())
    }
}

/// Resolve the prompt text from the CLI argument, reading from stdin when "-".
pub fn resolve_prompt_text(prompt_arg: &str) -> anyhow::Result<String> {
    resolve_prompt_text_from(prompt_arg, &mut std::io::stdin())
}
