//! CyberGuard agent binary — SPEC-001 (heartbeat) + SPEC-002 (enrollment).
//!
//! Wires the CLI (`--config <path>`), configuration loading, JSON logger
//! initialisation, the load-or-enroll identity step (SPEC-002, when an
//! `[enrollment]` block is configured), and the heartbeat run loop with
//! graceful shutdown on `Ctrl+C`.

use cg_agent::errors::EnrollmentError;
use cg_agent::{config, identity, init_logger_with_writer, log_lifecycle_event, run};
use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(name = "cg-agent", version, about = "CyberGuard endpoint agent")]
struct Cli {
    /// Path to the TOML configuration file. Defaults to `./agent.toml`.
    #[arg(long, value_name = "PATH", default_value = "agent.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    let mut cfg = match config::load_from_path(&cli.config) {
        Ok(c) => c,
        Err(e) => {
            // Logger is not initialised yet (FR-009); this is the
            // single place stderr carries agent output.
            eprintln!("cg-agent: invalid config: {e}");
            return ExitCode::from(2);
        }
    };

    if let Err(e) = init_logger_with_writer(&cfg.log.level, std::io::stdout()) {
        eprintln!("cg-agent: logger init failed: {e}");
        return ExitCode::from(1);
    }

    log_lifecycle_event("agent starting", "main");

    // SPEC-002: when an `[enrollment]` block is configured, resolve the
    // agent identity (load-or-enroll) before heartbeating, and adopt the
    // server-assigned `agent_id`. Without the block, run the SPEC-001
    // closed-test path unchanged.
    if cfg.enrollment.is_some() {
        match identity::ensure_identity(&cfg, &cli.config).await {
            Ok(id) => cfg.agent.id = id.agent_id,
            Err(e) => {
                report_enrollment_error(&e);
                return ExitCode::from(e.exit_code());
            }
        }
    }

    let shutdown = cg_agent::shutdown::wait_for_shutdown();
    match run(cfg, shutdown).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "agent exited with error");
            ExitCode::from(1)
        }
    }
}

/// Emit the SPEC-002 §Failure-modes stderr line for an enrollment error.
/// The logger is already initialised here, but the documented contract is
/// a single `cg-agent: ...` line on stderr, so we keep it explicit.
fn report_enrollment_error(err: &EnrollmentError) {
    match err {
        EnrollmentError::MissingToken => eprintln!("cg-agent: {err}"),
        EnrollmentError::Refused(msg) => eprintln!("cg-agent: enrollment failed: {msg}"),
        EnrollmentError::Unreachable { attempts, .. } => {
            eprintln!("cg-agent: enrollment failed: server unreachable after {attempts} attempts")
        }
        EnrollmentError::Persistence(msg) => eprintln!("cg-agent: identity error: {msg}"),
    }
}
