/// CLI argument definitions for coding-agent.
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "coding-agent", about = "A coding assistant agent")]
pub struct Cli {
    /// Run non-interactively with this prompt, then exit. Use "-" to read from stdin.
    #[arg(short, long)]
    pub prompt: Option<String>,

    /// Model ID (default: gpt-4o-mini, overrides OPENAI_MODEL env)
    #[arg(short, long)]
    pub model: Option<String>,

    /// Override the default system prompt
    #[arg(long)]
    pub system_prompt: Option<String>,

    /// Print human-readable performance stats to stderr at end of run
    #[arg(long)]
    pub stats: bool,

    /// Write JSON performance stats to this file at end of run
    #[arg(long, value_name = "PATH")]
    pub stats_json: Option<String>,
}
