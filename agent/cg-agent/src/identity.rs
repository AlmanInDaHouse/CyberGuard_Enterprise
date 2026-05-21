//! Load-or-enroll dispatcher and identity persistence.
//! See SPEC-002 §FR-001, §FR-007, §FR-008, §FR-014, §Behavior.

use crate::config::AgentConfig;
use crate::crypto::{pubkey_fingerprint, AgentKeypair, KEY_LEN};
use crate::errors::EnrollmentError;
use crate::secure_storage::default_store;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The on-disk `identity.json` (SPEC-002 §Data contracts).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedIdentity {
    pub agent_id: String,
    pub agent_pubkey_fingerprint: String,
    pub issued_at: String,
    pub expires_at: String,
}

/// A loaded, ready-to-use agent identity (in memory).
#[derive(Debug)]
pub struct Identity {
    pub agent_id: String,
    pub keypair: AgentKeypair,
    pub client_certificate_pem: String,
}

/// Startup dispatcher (SPEC-002 §Behavior). The single entry point the
/// agent (and the integration harness) calls after `LoadConfig` +
/// `InitLogger`:
///
/// - `CheckIdentity` (FR-001): if both `cert_path` and `key_path` exist,
///   `LoadIdentity` (FR-008) and return the loaded `Identity`.
/// - otherwise `Enrolling`: `enroll` (FR-003–FR-006) → `PersistIdentity`
///   (FR-007) → token hygiene (FR-014), returning the fresh `Identity`.
///
/// `config_path` is the path to `agent.toml`, needed for FR-014 token
/// hygiene after a successful first run. Returns the resolved identity
/// or an `EnrollmentError` whose `exit_code()` drives the process exit.
pub async fn ensure_identity(
    config: &AgentConfig,
    config_path: &Path,
) -> Result<Identity, EnrollmentError> {
    // A SPEC-002 startup requires the `[enrollment]` table to exist.
    let enr = config
        .enrollment
        .as_ref()
        .ok_or(EnrollmentError::MissingToken)?;

    if is_enrolled(config) {
        // FR-014 defensive: an identity is already present, so any token
        // still in the config is stale and must be ignored.
        if enr.token.as_deref().is_some_and(|t| !t.is_empty()) {
            tracing::warn!("stale enrollment token present in config, ignoring");
        }
        let identity = load_identity(config)?;
        tracing::info!(
            agent_id = %identity.agent_id,
            "identity loaded from disk"
        );
        return Ok(identity);
    }

    // Enrolling → PersistIdentity → token hygiene (FR-003 to FR-014).
    let enrolled = crate::enrollment::enroll(config).await?;
    persist_identity(config, &enrolled)?;
    tracing::info!(
        cert_path = %enr.cert_path,
        key_path = %enr.key_path,
        identity_path = %enr.identity_path,
        "identity persisted"
    );
    hygienize_token(config_path);

    let keypair = AgentKeypair::from_secret_bytes(&enrolled.secret_seed);
    Ok(Identity {
        agent_id: enrolled.agent_id,
        keypair,
        client_certificate_pem: enrolled.client_certificate_pem,
    })
}

/// CheckIdentity (FR-001): both cert and key files present ⇒ already
/// enrolled. Does not validate the cert; that happens in `load_identity`.
pub fn is_enrolled(config: &AgentConfig) -> bool {
    match config.enrollment.as_ref() {
        Some(enr) => Path::new(&enr.cert_path).exists() && Path::new(&enr.key_path).exists(),
        None => false,
    }
}

/// LoadIdentity (FR-008): read cert + decrypt key + cross-check the
/// pubkey fingerprint against `identity.json`. Any failure is exit code
/// `5` (carried by `EnrollmentError::Persistence`).
pub fn load_identity(config: &AgentConfig) -> Result<Identity, EnrollmentError> {
    let enr = config
        .enrollment
        .as_ref()
        .ok_or(EnrollmentError::MissingToken)?;

    let client_certificate_pem = std::fs::read_to_string(&enr.cert_path)
        .map_err(|e| EnrollmentError::Persistence(format!("cannot read cert.pem: {e}")))?;

    if !Path::new(&enr.identity_path).exists() {
        return Err(EnrollmentError::Persistence(
            "identity.json missing".to_string(),
        ));
    }
    let identity_raw = std::fs::read_to_string(&enr.identity_path)
        .map_err(|e| EnrollmentError::Persistence(format!("cannot read identity.json: {e}")))?;
    let persisted: PersistedIdentity = serde_json::from_str(&identity_raw)
        .map_err(|e| EnrollmentError::Persistence(format!("identity.json corrupted: {e}")))?;

    // Decrypt key.dat into a zeroized buffer, then rebuild the keypair.
    let sealed = std::fs::read(&enr.key_path)
        .map_err(|e| EnrollmentError::Persistence(format!("cannot read key.dat: {e}")))?;
    let secret =
        zeroize::Zeroizing::new(default_store().unseal(&sealed).map_err(|e| {
            EnrollmentError::Persistence(format!("cannot decrypt private key: {e}"))
        })?);
    if secret.len() != KEY_LEN {
        return Err(EnrollmentError::Persistence(format!(
            "decrypted key has wrong length ({} bytes)",
            secret.len()
        )));
    }
    let mut seed = zeroize::Zeroizing::new([0u8; KEY_LEN]);
    seed.copy_from_slice(&secret);
    let keypair = AgentKeypair::from_secret_bytes(&seed);

    // Tamper-evidence cross-check (FR-008): the derived pubkey must match
    // the fingerprint recorded at enrollment.
    let derived = pubkey_fingerprint(&keypair.public_key_bytes());
    if derived != persisted.agent_pubkey_fingerprint {
        return Err(EnrollmentError::Persistence(
            "pubkey fingerprint mismatch".to_string(),
        ));
    }

    Ok(Identity {
        agent_id: persisted.agent_id,
        keypair,
        client_certificate_pem,
    })
}

/// PersistIdentity (FR-007): write `cert.pem`, `key.dat` (sealed via the
/// platform `SecureStore`), and `identity.json`, then harden all three to
/// owner-only access (NFR-003). Any IO/ACL failure is exit code `5`.
pub fn persist_identity(
    config: &AgentConfig,
    enrolled: &crate::enrollment::EnrolledIdentity,
) -> Result<(), EnrollmentError> {
    let enr = config
        .enrollment
        .as_ref()
        .ok_or(EnrollmentError::MissingToken)?;

    // Ensure the target directory exists (no-op when it already does).
    for p in [&enr.cert_path, &enr.key_path, &enr.identity_path] {
        if let Some(parent) = Path::new(p).parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                EnrollmentError::Persistence(format!("cannot create {}: {e}", parent.display()))
            })?;
        }
    }

    let store = default_store();
    if !store.is_secure() {
        tracing::warn!(
            backend = store.backend_name(),
            "persisting private key with a NON-SECURE backend (test-only build)"
        );
    }
    let sealed = store
        .seal(&enrolled.secret_seed[..])
        .map_err(|e| EnrollmentError::Persistence(format!("cannot seal private key: {e}")))?;

    std::fs::write(&enr.cert_path, enrolled.client_certificate_pem.as_bytes())
        .map_err(|e| EnrollmentError::Persistence(format!("cannot write cert.pem: {e}")))?;
    std::fs::write(&enr.key_path, &sealed)
        .map_err(|e| EnrollmentError::Persistence(format!("cannot write key.dat: {e}")))?;

    let persisted = PersistedIdentity {
        agent_id: enrolled.agent_id.clone(),
        agent_pubkey_fingerprint: pubkey_fingerprint(&enrolled.public_key),
        issued_at: enrolled.issued_at.clone(),
        expires_at: enrolled.expires_at.clone(),
    };
    let identity_json = serde_json::to_string_pretty(&persisted).map_err(|e| {
        EnrollmentError::Persistence(format!("cannot serialize identity.json: {e}"))
    })?;
    std::fs::write(&enr.identity_path, identity_json)
        .map_err(|e| EnrollmentError::Persistence(format!("cannot write identity.json: {e}")))?;

    harden(Path::new(&enr.cert_path))?;
    harden(Path::new(&enr.key_path))?;
    harden(Path::new(&enr.identity_path))?;
    Ok(())
}

/// Token hygiene (FR-014): atomically rewrite `agent.toml` dropping
/// `enrollment.token`. Best-effort — a failure is logged at `warn` and
/// does not abort the run (the token is single-use and already consumed
/// server-side).
pub fn hygienize_token(config_path: &Path) {
    if let Err(e) = rewrite_without_token(config_path) {
        tracing::warn!(error = %e, "could not hygienize enrollment token from config");
    }
}

/// Rewrite `config_path` with `enrollment.token` removed, writing to a
/// sibling temp file and renaming over the original (atomic on the same
/// volume).
fn rewrite_without_token(config_path: &Path) -> Result<(), String> {
    let content = std::fs::read_to_string(config_path).map_err(|e| e.to_string())?;
    let mut doc: toml::Value = content
        .parse()
        .map_err(|e: toml::de::Error| e.to_string())?;

    if let Some(enr) = doc.get_mut("enrollment").and_then(|v| v.as_table_mut()) {
        enr.remove("token");
    }

    let rewritten = toml::to_string(&doc).map_err(|e| e.to_string())?;

    let mut tmp = config_path.to_path_buf();
    let file_name = config_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "agent.toml".to_string());
    tmp.set_file_name(format!("{file_name}.hygiene-tmp"));

    std::fs::write(&tmp, rewritten).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, config_path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Harden a persisted artifact to owner-only access (NFR-003).
///
/// Windows: strip inherited ACEs and grant full control only to the
/// current user and `SYSTEM` (well-known SID S-1-5-18), via `icacls`.
#[cfg(windows)]
fn harden(path: &Path) -> Result<(), EnrollmentError> {
    let owner = current_user_principal();
    let output = std::process::Command::new("icacls")
        .arg(path)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(format!("{owner}:(F)"))
        .arg("*S-1-5-18:(F)")
        .output()
        .map_err(|e| EnrollmentError::Persistence(format!("cannot run icacls: {e}")))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(EnrollmentError::Persistence(format!(
            "icacls failed on {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

/// `whoami` prints `DOMAIN\user` (or `COMPUTERNAME\user`), which `icacls`
/// accepts directly as a principal. Falls back to `%USERNAME%`.
#[cfg(windows)]
fn current_user_principal() -> String {
    std::process::Command::new("whoami")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| std::env::var("USERNAME").unwrap_or_default())
}

/// POSIX: mode `0600`. Parked for SPEC-003 Linux work (AC-012), but the
/// test-only non-Windows build still applies it so artifacts are never
/// world-readable.
#[cfg(unix)]
fn harden(path: &Path) -> Result<(), EnrollmentError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|e| {
        EnrollmentError::Persistence(format!("cannot set 0600 on {}: {e}", path.display()))
    })
}
