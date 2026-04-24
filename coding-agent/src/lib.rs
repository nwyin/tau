pub mod agent_builder;
pub mod cli;
pub mod config;
pub mod orchestration;
pub mod permissions;
pub mod rpc;
pub mod serve;
pub mod session;
pub mod skills;
pub mod system_prompt;
pub mod tools;
pub mod trace;
pub mod tui;

pub use tools::default_tools;

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
