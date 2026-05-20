//! CyberGuard agent binary — SPEC-001 (heartbeat).
//!
//! Wires the CLI (`--config <path>`), configuration loading, JSON
//! logger initialisation, and the heartbeat run loop, with graceful
//! shutdown on `Ctrl+C` per SPEC-001 §Behavior.

use cg_agent::{config, init_logger_with_writer, log_lifecycle_event, run};
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "cg-agent", version, about = "CyberGuard endpoint agent")]
struct Cli {
    /// Path to the TOML configuration file. Defaults to `./agent.toml`.
    #[arg(long, value_name = "PATH", default_value = "agent.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();

    let cfg = match config::load_from_path(&cli.config) {
        Ok(c) => c,
        Err(e) => {
            // Logger is not initialised yet (FR-009); this is the
            // single place stderr carries agent output.
            eprintln!("cg-agent: invalid config: {e}");
            return std::process::ExitCode::from(2);
        }
    };

    if let Err(e) = init_logger_with_writer(&cfg.log.level, std::io::stdout()) {
        eprintln!("cg-agent: logger init failed: {e}");
        return std::process::ExitCode::from(1);
    }

    log_lifecycle_event("agent starting", "main");

    let shutdown = cg_agent::shutdown::wait_for_shutdown();
    match run(cfg, shutdown).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "agent exited with error");
            std::process::ExitCode::from(1)
        }
    }
}
