//! The agent as a Windows service (SPEC-024 §Data contracts §3,
//! §Operational §3 and its second Amendment 2026-11-01, items 2 to 4).
//!
//! `--service` runs only when the service control manager (SCM) started
//! the process; otherwise, and on a platform without an SCM, the agent
//! exits with code 11 before it reads its configuration. Under the SCM the
//! service runs the console's secure runtime: `START_PENDING` until the
//! configuration is valid, the instance lock is taken and the file log is
//! open; then `RUNNING`, accepting `STOP` and `SHUTDOWN` alone, before the
//! identity step, enrollment and capture. A stop has 60 seconds in all;
//! `STOPPED` carries code 0 after a clean stop, else
//! `ERROR_SERVICE_SPECIFIC_ERROR` and the cause's code. The Application log
//! gets entries 1000 to 1003 under the source `CyberGuardAgent`. A start
//! asked to by its SCM start arguments migrates a console installation
//! during `START_PENDING` (`migration`).
//!
//! This module holds what is portable: the codes, the lines, the outcome
//! and its entry, and the enrollment rounds. The SCM, the Application log
//! and the file log are in `windows`.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::ExitCode;
use std::time::Duration;

use crate::config::AgentConfig;
use crate::errors::{ConfigError, EnrollmentError};
use crate::identity::Identity;

#[cfg(windows)]
pub mod layout;
#[cfg(windows)]
mod log_file;
#[cfg(windows)]
pub mod migration;
pub mod rotation;
#[cfg(windows)]
mod windows;

/// `--service` without a start by the SCM (item 2).
pub const EXIT_NOT_STARTED_BY_SCM: u8 = 11;
/// The stop deadline expired (item 3).
pub const EXIT_STOP_DEADLINE: u8 = 12;
/// The service's file log cannot be written (item 3).
pub const EXIT_SERVICE_LOG: u8 = 13;

/// Item 2's line for `--service` without a start by the SCM.
pub const STDERR_NOT_STARTED_BY_SCM: &str =
    "cg-agent: --service requires a start by the Windows service control manager";
/// Item 2's line for a `--config` that is not absolute in service mode.
pub const STDERR_RELATIVE_CONFIG: &str =
    "cg-agent: invalid config: --config must be an absolute path in service mode";

/// Item 3's line for a file log that cannot be written.
pub fn stderr_service_log(reason: &str) -> String {
    format!("cg-agent: the service log cannot be written: {reason}")
}

/// The service's stop deadline, from the STOP control: teardown and the
/// final request together (SPEC-024 §Operational §3).
pub const STOP_DEADLINE: Duration = Duration::from_secs(60);

/// The run's own budget from the moment it sees the stop: inside the
/// deadline, so that the run reports what was pending and how many events
/// it lost before the watchdog has to end it without knowing.
pub const STOP_BUDGET: Duration = Duration::from_secs(58);

/// What was pending when the watchdog, not the run, ended the stop.
pub const PENDING_UNKNOWN: &str = "the agent's stop, at a point the run did not report";

/// The wait between two exhausted enrollment rounds in service mode
/// (SPEC-002 Amendment 2026-10-27: at least 60 seconds).
pub const ENROLLMENT_ROUND_WAIT: Duration = Duration::from_secs(60);

/// The Application-log source (item 4).
pub const EVENT_SOURCE: &str = "CyberGuardAgent";
/// Information: the service reports `RUNNING`, with its version.
pub const EVENT_RUNNING: u32 = 1000;
/// Information: the service stopped cleanly.
pub const EVENT_STOPPED: u32 = 1001;
/// Error: the service stopped on a failure other than the deadline.
pub const EVENT_FAILED: u32 = 1002;
/// Error: the stop deadline expired.
pub const EVENT_DEADLINE: u32 = 1003;

/// How the service ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceExit {
    /// A clean stop; `undelivered` events were lost on the way out.
    Clean { undelivered: usize },
    /// A failure, with the code the console would exit with (or 13) and
    /// the cause's line.
    Failed { code: u8, line: String },
    /// The stop deadline expired with `pending` still to do; `undelivered`
    /// is `None` when the run could not count the events it lost.
    DeadlineExpired {
        pending: &'static str,
        undelivered: Option<usize>,
    },
}

/// One Application-log entry: its id, whether it is an error, its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: u32,
    pub error: bool,
    pub text: String,
}

impl ServiceExit {
    /// The service-specific exit code `STOPPED` carries, or `None` for a
    /// clean stop (`dwWin32ExitCode` 0).
    pub fn specific_code(&self) -> Option<u8> {
        match self {
            ServiceExit::Clean { .. } => None,
            ServiceExit::Failed { code, .. } => Some(*code),
            ServiceExit::DeadlineExpired { .. } => Some(EXIT_STOP_DEADLINE),
        }
    }

    /// The Application-log entry that records this end (item 4).
    pub fn entry(&self) -> Entry {
        match self {
            ServiceExit::Clean { undelivered } => Entry {
                id: EVENT_STOPPED,
                error: false,
                text: format!(
                    "cg-agent: the service stopped cleanly; events not delivered: {undelivered}"
                ),
            },
            ServiceExit::Failed { code, line } => Entry {
                id: EVENT_FAILED,
                error: true,
                text: format!("cg-agent: the service stopped with code {code}: {line}"),
            },
            ServiceExit::DeadlineExpired {
                pending,
                undelivered,
            } => Entry {
                id: EVENT_DEADLINE,
                error: true,
                text: format!(
                    "cg-agent: the service stop could not end within its {} s deadline, with \
                     {pending} pending; events not delivered: {}",
                    STOP_DEADLINE.as_secs(),
                    undelivered.map_or_else(|| "unknown".to_string(), |n| n.to_string())
                ),
            },
        }
    }
}

/// A file log that stopped writing turns a clean stop into code 13 with
/// its line (item 3); any other end keeps its own code.
pub fn with_log_failure(exit: ServiceExit, log_failure: Option<&str>) -> ServiceExit {
    match (log_failure, exit) {
        (Some(reason), ServiceExit::Clean { .. }) => ServiceExit::Failed {
            code: EXIT_SERVICE_LOG,
            line: stderr_service_log(reason),
        },
        (_, exit) => exit,
    }
}

/// The entry 1000 text: the service runs, with its version.
pub fn running_entry() -> Entry {
    Entry {
        id: EVENT_RUNNING,
        error: false,
        text: format!("cg-agent {}: the service is running", crate::AGENT_VERSION),
    }
}

/// A configuration error's line for the service (item 4): it names the
/// file and, for a parse error, the parser's line and column, and quotes
/// no line of the file, which can carry a token. The other errors' texts
/// carry no value of the file.
pub fn config_error_line(path: &Path, error: &ConfigError) -> String {
    let detail = match error {
        ConfigError::Parse {
            position: Some((line, column)),
            ..
        } => format!("TOML parse error at line {line}, column {column}"),
        ConfigError::Parse { position: None, .. } => "TOML parse error".to_string(),
        other => other.to_string(),
    };
    format!("cg-agent: invalid config {}: {detail}", path.display())
}

/// The installed service runs the secure path alone (SPEC-024 §Data
/// contracts §2 and §Operational §3: no silent plain HTTP): its
/// configuration has `[enrollment]` and a trust anchor, which the loader
/// already ties to `https://` URLs. The error is the cause's line.
pub fn check_service_config(path: &Path, config: &AgentConfig) -> Result<(), String> {
    if config.enrollment.is_none() || config.server.trust_anchor_path.is_none() {
        return Err(format!(
            "cg-agent: invalid config {}: the service requires [enrollment] and server.trust_anchor_path",
            path.display()
        ));
    }
    Ok(())
}

/// The installed paths (SPEC-024 §Data contracts §2 and its Amendment
/// 2026-11-04, item 1), under the `%ProgramData%` the OS reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPaths {
    /// `<ProgramData>\CyberGuard`.
    pub product_root: PathBuf,
    /// `<ProgramData>\CyberGuard\agent`.
    pub data_root: PathBuf,
    pub config: PathBuf,
    pub trust_anchor: PathBuf,
    /// `<ProgramData>\CyberGuard\agent\identity`.
    pub identity_dir: PathBuf,
    pub cert: PathBuf,
    pub key: PathBuf,
    pub identity: PathBuf,
}

impl InstalledPaths {
    pub fn under(program_data: &Path) -> Self {
        let product_root = program_data.join("CyberGuard");
        let data_root = product_root.join("agent");
        let identity_dir = data_root.join("identity");
        InstalledPaths {
            config: data_root.join("agent.toml"),
            trust_anchor: data_root.join("server-ca.pem"),
            cert: identity_dir.join("cert.pem"),
            key: identity_dir.join("key.dat"),
            identity: identity_dir.join("identity.json"),
            product_root,
            data_root,
            identity_dir,
        }
    }

    /// FR-014's temporary file in the data root.
    pub fn hygiene_temporary(&self) -> PathBuf {
        self.data_root.join("agent.toml.hygiene-tmp")
    }

    /// `agent.migrating` beside the data root, the migration's staging
    /// directory (Amendment 2026-11-05).
    pub fn staging(&self) -> PathBuf {
        self.product_root.join("agent.migrating")
    }

    /// `agent.legacy` beside the data root, the console installation's
    /// directory after the migration's commit.
    pub fn legacy(&self) -> PathBuf {
        self.product_root.join("agent.legacy")
    }

    /// `migration.json` in the data root, after the commit.
    pub fn record(&self) -> PathBuf {
        self.data_root.join("migration.json")
    }
}

/// A configured path is its fixed path when the two strings are equal but
/// for ASCII case; no other spelling is resolved (Amendment 2026-11-04,
/// item 2).
pub fn is_fixed_path(configured: &str, fixed: &Path) -> bool {
    fixed
        .to_str()
        .is_some_and(|fixed| fixed.eq_ignore_ascii_case(configured))
}

/// Item 2's line for a path other than its fixed one.
fn not_fixed(config_path: &Path, key: &str, fixed: &Path) -> String {
    format!(
        "cg-agent: invalid config {}: {key} must be {} in service mode",
        config_path.display(),
        fixed.display()
    )
}

/// Before the configuration is read: `--config` is the fixed one.
pub fn check_config_path(paths: &InstalledPaths, config_path: &Path) -> Result<(), String> {
    match config_path.to_str() {
        Some(text) if is_fixed_path(text, &paths.config) => Ok(()),
        _ => Err(not_fixed(config_path, "--config", &paths.config)),
    }
}

/// After parsing: the trust anchor and the identity at their fixed paths,
/// and an explicit heartbeat URL (Amendment 2026-11-04, item 2). The plain
/// path is refused first, by `check_service_config`.
pub fn check_installed_config(
    paths: &InstalledPaths,
    config_path: &Path,
    config: &AgentConfig,
) -> Result<(), String> {
    check_service_config(config_path, config)?;
    let anchor = config.server.trust_anchor_path.as_deref().unwrap_or("");
    if !is_fixed_path(anchor, &paths.trust_anchor) {
        return Err(not_fixed(
            config_path,
            "server.trust_anchor_path",
            &paths.trust_anchor,
        ));
    }
    let enrollment = config.enrollment.as_ref().expect("checked above");
    for (key, configured, fixed) in [
        ("enrollment.cert_path", &enrollment.cert_path, &paths.cert),
        ("enrollment.key_path", &enrollment.key_path, &paths.key),
        (
            "enrollment.identity_path",
            &enrollment.identity_path,
            &paths.identity,
        ),
    ] {
        if !is_fixed_path(configured, fixed) {
            return Err(not_fixed(config_path, key, fixed));
        }
    }
    if config.server.heartbeat_url.is_none() {
        return Err(format!(
            "cg-agent: invalid config {}: the service requires server.heartbeat_url",
            config_path.display()
        ));
    }
    Ok(())
}

/// Load or enroll the identity in service mode (SPEC-002 Amendment
/// 2026-10-27): an exhausted enrollment round (the console's code 4) is
/// followed by another after `round_wait`, not by an exit; any other error
/// ends it. `stop` resolving at any point — a request in flight, a retry's
/// backoff or the wait between rounds — ends it at once with `None`.
pub async fn ensure_identity_until_stopped<S>(
    config: &AgentConfig,
    config_path: &Path,
    round_wait: Duration,
    stop: Pin<&mut S>,
) -> Option<Result<Identity, EnrollmentError>>
where
    S: Future<Output = ()>,
{
    identity_rounds_until_stopped(
        || crate::identity::ensure_identity(config, config_path),
        round_wait,
        stop,
    )
    .await
}

/// The rounds of `ensure_identity_until_stopped`, each one `round()`: the
/// service's rounds run installed mode's dispatcher.
pub async fn identity_rounds_until_stopped<S, R, F>(
    mut round: R,
    round_wait: Duration,
    mut stop: Pin<&mut S>,
) -> Option<Result<Identity, EnrollmentError>>
where
    S: Future<Output = ()>,
    R: FnMut() -> F,
    F: Future<Output = Result<Identity, EnrollmentError>>,
{
    loop {
        let result = tokio::select! {
            biased;
            _ = stop.as_mut() => return None,
            result = round() => result,
        };
        match result {
            Err(EnrollmentError::Unreachable {
                attempts,
                last_error,
            }) => {
                tracing::warn!(
                    attempts,
                    last_error = %last_error,
                    wait_seconds = round_wait.as_secs(),
                    "enrollment round exhausted; the next round waits"
                );
                tokio::select! {
                    biased;
                    _ = stop.as_mut() => return None,
                    _ = tokio::time::sleep(round_wait) => {}
                }
            }
            other => return Some(other),
        }
    }
}

/// Run as the Windows service: hand the process to the SCM's dispatcher.
/// Without a start by the SCM, exit 11 with item 2's line, before the
/// configuration is read.
pub fn run(config_path: std::path::PathBuf) -> ExitCode {
    #[cfg(windows)]
    {
        windows::run(config_path)
    }
    #[cfg(not(windows))]
    {
        let _ = config_path;
        eprintln!("{STDERR_NOT_STARTED_BY_SCM}");
        ExitCode::from(EXIT_NOT_STARTED_BY_SCM)
    }
}
