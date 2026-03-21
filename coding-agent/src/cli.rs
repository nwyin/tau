/// CLI argument definitions for coding-agent.
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "tau", about = "A coding assistant agent")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

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

    /// Resume a specific session by ID
    #[arg(long, value_name = "ID")]
    pub session: Option<String>,

    /// Resume the most recent session
    #[arg(long, conflicts_with = "session")]
    pub resume: bool,

    /// Run without session persistence (ephemeral)
    #[arg(long, conflicts_with_all = ["session", "resume"])]
    pub no_session: bool,

    /// Comma-separated list of tools to enable (overrides config)
    #[arg(long, value_delimiter = ',')]
    pub tools: Option<Vec<String>>,

    /// Directory for trace output (run.json + trace.jsonl)
    #[arg(long, value_name = "DIR")]
    pub trace_output: Option<String>,

    /// Task ID for benchmark identification
    #[arg(long, value_name = "ID")]
    pub task_id: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Start a JSON-RPC server on stdio (one session per process).
    /// Used as a backend for orchestrators like Hive.
    Serve {
        /// Working directory for this session
        #[arg(long, default_value = ".")]
        cwd: String,

        /// Model ID (overrides TAU_MODEL env and config)
        #[arg(long)]
        model: Option<String>,

        /// Comma-separated list of tools to enable
        #[arg(long, value_delimiter = ',')]
        tools: Option<Vec<String>>,
    },

    /// List all available models.
    Models {
        /// Filter models by provider (e.g. "openrouter", "anthropic", "openai")
        #[arg(long, short)]
        provider: Option<String>,
    },
}
