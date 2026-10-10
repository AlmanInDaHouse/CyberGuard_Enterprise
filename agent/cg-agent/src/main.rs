//! CyberGuard agent binary — SPEC-001 (heartbeat), SPEC-002 (enrollment),
//! SPEC-003 (mTLS 1.3 + signed envelope) and SPEC-017 (process capture on
//! the secure path).
//!
//! Wires the CLI (`--config <path>`), configuration loading, JSON logger
//! initialisation, the load-or-enroll identity step (SPEC-002), the
//! secure path (SPEC-003 + SPEC-017, when `server.trust_anchor_path` is
//! set), and graceful shutdown on `Ctrl+C`.

use cg_agent::errors::{AgentError, EnrollmentError, TlsError};
use cg_agent::{
    config, identity, init_logger_with_writer, log_lifecycle_event, run, run_secure, Capture,
};
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

    let shutdown = cg_agent::shutdown::wait_for_shutdown();

    // SPEC-003 secure path: TLS 1.3 mTLS + signed envelope, using the
    // SPEC-002 identity, with process capture on Windows (SPEC-017): the
    // identity is loaded or enrolled first, then run_secure opens the ETW
    // session. Requires enrollment to have produced an identity.
    if cfg.server.trust_anchor_path.is_some() {
        let id = match identity::ensure_identity(&cfg, &cli.config).await {
            Ok(id) => id,
            Err(e) => {
                report_enrollment_error(&e);
                return ExitCode::from(e.exit_code());
            }
        };
        cfg.agent.id = id.agent_id.clone();
        return match run_secure(cfg, id, Capture::Platform, shutdown).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                report_agent_error(&e);
                ExitCode::from(e.exit_code())
            }
        };
    }

    // SPEC-002 path: enroll (if configured) then the SPEC-001 plain-HTTP
    // heartbeat, adopting the server-assigned agent_id.
    if cfg.enrollment.is_some() {
        match identity::ensure_identity(&cfg, &cli.config).await {
            Ok(id) => cfg.agent.id = id.agent_id,
            Err(e) => {
                report_enrollment_error(&e);
                return ExitCode::from(e.exit_code());
            }
        }
    }

    // SPEC-001 plain path.
    match run(cfg, shutdown).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "agent exited with error");
            ExitCode::from(1)
        }
    }
}

/// SPEC-002 §Failure-modes stderr line for an enrollment error.
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

/// SPEC-003 / SPEC-017 §Failure-modes stderr line for a secure-path error.
fn report_agent_error(err: &AgentError) {
    match err {
        AgentError::Tls(TlsError::ServerCertUntrusted(m)) => {
            eprintln!("cg-agent: tls: server certificate verification failed: {m}")
        }
        AgentError::Tls(TlsError::ClientConfig(m)) => {
            eprintln!("cg-agent: tls: client configuration failed: {m}")
        }
        AgentError::Tls(TlsError::ClientCertRejected(_)) => {
            eprintln!("cg-agent: tls: server rejected client certificate")
        }
        AgentError::Signing(s) => eprintln!("cg-agent: signing failed: {s}"),
        // SPEC-017 §Operational §1: the line goes to stderr and to the log.
        AgentError::Etw(etw) => {
            let line = etw.stderr_line();
            tracing::error!(exit_code = etw.exit_code(), "{line}");
            eprintln!("{line}");
        }
        // SPEC-020 §Operational §6: the same, for the logon subscription.
        AgentError::Logon(logon) => {
            let line = logon.stderr_line();
            tracing::error!(exit_code = logon.exit_code(), "{line}");
            eprintln!("{line}");
        }
        other => eprintln!("cg-agent: {other}"),
    }
}
