//! `orbit`: the command-line client. Imports history into a running Orbit server.

mod client;
mod import;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "orbit", about = "Orbit command-line client")]
struct Cli {
    /// Orbit server URL.
    #[arg(long, env = "ORBIT_URL", default_value = "http://127.0.0.1:8787", global = true)]
    url: String,
    /// Bearer token for the server.
    #[arg(long, env = "ORBIT_TOKEN", hide_env_values = true, global = true)]
    token: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Import an agent's session history.
    Import {
        #[command(subcommand)]
        from: Source,
        /// Parse and report, without sending anything.
        #[arg(long, global = true)]
        dry_run: bool,
        /// Tool output above this size is skipped, never truncated.
        #[arg(long, default_value_t = 16 * 1024, global = true)]
        max_tool_output_bytes: usize,
    },
}

#[derive(Subcommand, Clone)]
pub enum Source {
    /// Claude Code sessions (default: ~/.claude/projects).
    ClaudeCode {
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Codex sessions (default: ~/.codex/sessions).
    Codex {
        #[arg(long)]
        dir: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Import {
            from,
            dry_run,
            max_tool_output_bytes,
        } => {
            let options = orbit_import::Options { max_tool_output_bytes };
            let client = if dry_run {
                None
            } else {
                let token = cli.token.ok_or_else(|| anyhow::anyhow!("ORBIT_TOKEN is not set"))?;
                Some(client::Client::new(cli.url, token))
            };
            import::run(&from, &options, client.as_ref()).await
        }
    }
}
