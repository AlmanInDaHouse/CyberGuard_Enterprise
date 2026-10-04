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

/// TLS / mutual-auth handshake errors on the secure heartbeat path
/// (SPEC-003 §Failure modes). Terminal; each maps to a documented exit
/// code via `exit_code()`. Transient TLS failures (reset, timeout) are
/// reported through `TransportError` and retried, not represented here.
#[derive(Debug, Error)]
pub enum TlsError {
    /// The server certificate did not validate against the configured
    /// trust anchor (untrusted chain, expired, hostname mismatch). The
    /// agent fails closed. Exit 6.
    #[error("server certificate verification failed: {0}")]
    ServerCertUntrusted(String),

    /// The server rejected our client certificate at the TLS layer
    /// (unknown / revoked / expired). Operator must re-enroll. Exit 7.
    #[error("server rejected client certificate: {0}")]
    ClientCertRejected(String),

    /// The rustls `ClientConfig` could not be built (bad trust-anchor PEM,
    /// unusable client key). A local configuration fault. Exit 6.
    #[error("tls client configuration failed: {0}")]
    ClientConfig(String),
}

impl TlsError {
    /// Process exit code per SPEC-003 §Failure modes.
    pub fn exit_code(&self) -> u8 {
        match self {
            TlsError::ServerCertUntrusted(_) | TlsError::ClientConfig(_) => 6,
            TlsError::ClientCertRejected(_) => 7,
        }
    }
}

/// Local signing / canonicalization failure (SPEC-003 §Failure modes).
/// Terminal. Exit 8.
#[derive(Debug, Error)]
pub enum SigningError {
    #[error("canonicalization failed: {0}")]
    Canonical(String),

    #[error("signature operation failed: {0}")]
    Sign(String),
}

impl SigningError {
    /// Process exit code per SPEC-003 §Failure modes.
    pub fn exit_code(&self) -> u8 {
        8
    }
}

/// The exact stderr line for a privilege failure (SPEC-005 AC-002,
/// ADR-0010 §Decision part 1).
pub const STDERR_INSUFFICIENT_PRIVILEGE: &str = "cg-agent: insufficient privilege to open \
     Microsoft-Windows-Kernel-Process ETW session; run as elevated user or LocalSystem";

/// ETW session-start failure (SPEC-017 §Operational §1, SPEC-005
/// §Failure modes, ADR-0010 §Decision part 1). Terminal at startup:
/// exit code 9 for a privilege failure, 1 for any other.
#[derive(Debug, Error)]
pub enum EtwError {
    #[error("ETW open refused: insufficient privilege")]
    PrivilegeNotHeld,

    #[error("ETW open refused: access denied")]
    AccessDenied,

    #[error("ETW session open failed: {code} {message}")]
    Failed { code: u32, message: String },

    #[error("ETW capture is not available on this platform")]
    Unsupported,
}

impl EtwError {
    /// Process exit code: 9 for a privilege failure (ADR-0010 §Decision
    /// part 1), 1 otherwise (SPEC-005 §Failure modes).
    pub fn exit_code(&self) -> u8 {
        match self {
            EtwError::PrivilegeNotHeld | EtwError::AccessDenied => 9,
            EtwError::Failed { .. } | EtwError::Unsupported => 1,
        }
    }

    /// The single stderr line written before the process exits.
    pub fn stderr_line(&self) -> String {
        match self {
            EtwError::PrivilegeNotHeld | EtwError::AccessDenied => {
                STDERR_INSUFFICIENT_PRIVILEGE.to_string()
            }
            other => format!("cg-agent: {other}"),
        }
    }
}

impl From<crate::etw::OpenError> for EtwError {
    fn from(err: crate::etw::OpenError) -> Self {
        match err {
            crate::etw::OpenError::PrivilegeNotHeld => EtwError::PrivilegeNotHeld,
            crate::etw::OpenError::AccessDenied => EtwError::AccessDenied,
            crate::etw::OpenError::Failed { code, message } => EtwError::Failed { code, message },
            crate::etw::OpenError::Unsupported => EtwError::Unsupported,
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

    #[error(transparent)]
    Tls(#[from] TlsError),

    #[error(transparent)]
    Signing(#[from] SigningError),

    #[error(transparent)]
    Etw(#[from] EtwError),
}

impl AgentError {
    /// The process exit code for a terminal agent error (SPEC-001/002/003
    /// §Failure modes, SPEC-005 exit code 9).
    pub fn exit_code(&self) -> u8 {
        match self {
            AgentError::Tls(t) => t.exit_code(),
            AgentError::Signing(s) => s.exit_code(),
            AgentError::Enrollment(en) => en.exit_code(),
            AgentError::Config(_) => 2,
            AgentError::Etw(e) => e.exit_code(),
            _ => 1,
        }
    }
}
