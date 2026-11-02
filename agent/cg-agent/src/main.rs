//! CyberGuard agent binary — SPEC-001 (heartbeat), SPEC-002 (enrollment),
//! SPEC-003 (mTLS 1.3 + signed envelope) and SPEC-017 (process capture on
//! the secure path).
//!
//! Wires the CLI (`--config <path>`), configuration loading, JSON logger
//! initialisation, the load-or-enroll identity step (SPEC-002, over TLS
//! since SPEC-023), the secure path (SPEC-003 + SPEC-017, when
//! `server.trust_anchor_path` is set, as it always is with
//! `[enrollment]`), and graceful shutdown on `Ctrl+C`.

use cg_agent::errors::{AgentError, EnrollmentError, TlsError};
use cg_agent::instance_lock::{self, LockOutcome};
use cg_agent::{
    config, identity, init_logger_with_writer, log_agent_starting, run, run_secure, Capture,
    AGENT_VERSION,
};
use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser, Debug)]
// SPEC-024 §Data contracts §1: the product version, fixed at build time.
#[command(name = "cg-agent", version = AGENT_VERSION, about = "CyberGuard endpoint agent")]
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

    log_agent_starting();

    let shutdown = cg_agent::shutdown::wait_for_shutdown();

    // SPEC-003 secure path: TLS 1.3 mTLS + signed envelope, using the
    // SPEC-002 identity, with process capture on Windows (SPEC-017): the
    // identity is loaded or enrolled first, then run_secure opens the ETW
    // session. Requires enrollment to have produced an identity.
    if cfg.server.trust_anchor_path.is_some() {
        // SPEC-024 §1: take the per-host instance lock before the identity
        // step, so before the capture start reclaims the agent's fixed ETW
        // session. The guard `_instance_lock` lives to the end of this
        // branch — across the whole secure run — and frees the lock when it
        // drops at the return. A token that controls trace sessions outside
        // ADR-0010's model is refused here (code 1); a second live instance
        // exits code 10 before enrolling; an ordinary/unelevated token goes
        // on without the lock (and so without the reclaim).
        let _instance_lock;
        let holds_lock;
        match instance_lock::acquire() {
            LockOutcome::Acquired(lock) => {
                _instance_lock = Some(lock);
                holds_lock = true;
            }
            LockOutcome::Unprivileged => {
                _instance_lock = None;
                holds_lock = false;
            }
            outcome => return report_lock_outcome(&outcome),
        }

        let id = match identity::ensure_identity(&cfg, &cli.config).await {
            Ok(id) => id,
            Err(e) => {
                report_enrollment_error(&e);
                return ExitCode::from(e.exit_code());
            }
        };
        cfg.agent.id = id.agent_id.clone();
        return match run_secure(cfg, id, Capture::Platform { holds_lock }, shutdown).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                report_agent_error(&e);
                ExitCode::from(e.exit_code())
            }
        };
    }

    // SPEC-001 plain path: no [enrollment] and no trust anchor. An enrolled
    // agent always has a trust anchor (SPEC-023 §Operational §6), so it
    // heartbeats on the secure path above.
    match run(cfg, shutdown).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "agent exited with error");
            ExitCode::from(1)
        }
    }
}

/// SPEC-024 §1: report an instance-lock outcome that ends the start
/// (`Held`, `ControlsEtwOutsideModel` or `Failed`) and return its exit
/// code. The line goes to stderr and, for the service's later file/Event
/// log, to the tracing log too (as SPEC-017 does for ETW errors). Never
/// called for `Acquired`/`Unprivileged`, which go on.
fn report_lock_outcome(outcome: &LockOutcome) -> ExitCode {
    let code = outcome
        .exit_code()
        .expect("report_lock_outcome is only called for an outcome that ends the start");
    let line = outcome
        .stderr_line()
        .expect("an outcome that ends the start has a line");
    tracing::error!(exit_code = code, "{line}");
    eprintln!("{line}");
    ExitCode::from(code)
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
        EnrollmentError::Tls(tls) => report_tls_error(tls),
    }
}

/// SPEC-003 §Failure-modes stderr line for a TLS error, on enrollment's
/// client or the heartbeat's.
fn report_tls_error(err: &TlsError) {
    match err {
        TlsError::ServerCertUntrusted(m) => {
            eprintln!("cg-agent: tls: server certificate verification failed: {m}")
        }
        TlsError::ClientConfig(m) => eprintln!("cg-agent: tls: client configuration failed: {m}"),
        TlsError::ClientCertRejected(_) => {
            eprintln!("cg-agent: tls: server rejected client certificate")
        }
    }
}

/// SPEC-003 / SPEC-017 §Failure-modes stderr line for a secure-path error.
fn report_agent_error(err: &AgentError) {
    match err {
        AgentError::Tls(tls) => report_tls_error(tls),
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
