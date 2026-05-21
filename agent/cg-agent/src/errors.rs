//! Domain error types for `cg-agent`. See SPEC-001 §Failure modes.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file not found: {0}")]
    NotFound(String),

    #[error("failed to read config file: {0}")]
    Io(String),

    #[error("failed to parse config TOML: {0}")]
    Parse(String),

    #[error("invalid config: missing key '{0}'")]
    MissingKey(String),

    #[error("invalid config: {0}")]
    Invalid(String),
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("transport error: {0}")]
    Network(String),

    #[error("server returned non-2xx: {status}")]
    BadStatus { status: u16 },

    #[error("retry budget exhausted after {attempts} attempts: {last_error}")]
    RetryExhausted { attempts: u32, last_error: String },

    #[error("request timed out after {timeout_ms} ms")]
    Timeout { timeout_ms: u64 },
}

/// At-rest secure-storage errors (SPEC-002 §Security considerations).
#[derive(Debug, Error)]
pub enum SecureStoreError {
    #[error("seal failed: {0}")]
    Seal(String),

    #[error("unseal failed (wrong machine, or corrupted blob): {0}")]
    Unseal(String),
}

/// Enrollment errors (SPEC-002 §Failure modes). Each variant maps to a
/// documented exit code; see `exit_code()`.
#[derive(Debug, Error)]
pub enum EnrollmentError {
    /// First-run enrollment was requested but `enrollment.token` is
    /// absent or empty (FR-002). Shares the config-error exit code (2),
    /// and renders the same `missing key '<path>'` substring as
    /// `ConfigError::MissingKey` so the stderr contract is uniform.
    #[error("invalid config: missing key 'enrollment.token'")]
    MissingToken,

    /// Server refused the token (401/403), token already used (409),
    /// or returned a malformed body. Terminal — no retry. Exit 3.
    #[error("enrollment refused: {0}")]
    Refused(String),

    /// Server 5xx or network failure after exhausting retries. Exit 4.
    #[error("server unreachable after {attempts} attempts: {last_error}")]
    Unreachable { attempts: u32, last_error: String },

    /// Could not persist or load identity artifacts. Exit 5.
    #[error("identity persistence/load failed: {0}")]
    Persistence(String),
}

impl EnrollmentError {
    /// Process exit code per SPEC-002 §Failure modes.
    pub fn exit_code(&self) -> u8 {
        match self {
            EnrollmentError::MissingToken => 2,
            EnrollmentError::Refused(_) => 3,
            EnrollmentError::Unreachable { .. } => 4,
            EnrollmentError::Persistence(_) => 5,
        }
    }
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error(transparent)]
    Transport(#[from] TransportError),

    #[error(transparent)]
    Enrollment(#[from] EnrollmentError),

    #[error(transparent)]
    SecureStore(#[from] SecureStoreError),
}
