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

/// SPEC-002 Amendment 2026-10-27's identity-set check: only an absent
/// identity set starts enrollment, and a set with some of its three files
/// but not all fails closed — no enrollment, no heartbeat, nothing
/// overwritten — with the repair action. Exit code 5. The service runs
/// `installed::check`, which refines it (SPEC-024 Amendment 2026-11-04,
/// items 3 and 4); the console keeps FR-001's check.
pub fn check_identity_set(config: &AgentConfig) -> Result<(), EnrollmentError> {
    let Some(enr) = config.enrollment.as_ref() else {
        return Ok(());
    };
    let files = [
        ("the certificate", &enr.cert_path),
        ("the key", &enr.key_path),
        ("identity.json", &enr.identity_path),
    ];
    let mut missing = Vec::new();
    for (name, path) in &files {
        // A file that cannot be inspected is not taken as absent: that
        // would enroll over it.
        match Path::new(path).try_exists() {
            Ok(true) => {}
            Ok(false) => missing.push(*name),
            Err(e) => {
                return Err(EnrollmentError::Persistence(format!(
                    "cannot inspect {name} at {path}: {e}"
                )))
            }
        }
    }
    if missing.is_empty() || missing.len() == files.len() {
        return Ok(());
    }
    Err(EnrollmentError::Persistence(format!(
        "incomplete identity: {} missing; restore the identity, or remove all three files \
         and enroll with a new token",
        missing.join(" and ")
    )))
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
    let rewritten = without_token(&content)?;

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

/// `agent.toml`'s text with `enrollment.token` removed.
fn without_token(content: &str) -> Result<String, String> {
    let mut doc: toml::Value = content
        .parse()
        .map_err(|e: toml::de::Error| e.to_string())?;

    if let Some(enr) = doc.get_mut("enrollment").and_then(|v| v.as_table_mut()) {
        enr.remove("token");
    }

    toml::to_string(&doc).map_err(|e| e.to_string())
}

/// Installed mode's mutual consistency (SPEC-024 §Operational §2 and its
/// Amendment 2026-11-04, item 5): `cert_pem`'s Ed25519 key is
/// `public_key` and its subject common name is `agent_id`. The error
/// names the field that differs, never a value.
pub fn check_consistency(
    cert_pem: &str,
    public_key: &[u8; KEY_LEN],
    agent_id: &str,
) -> Result<(), String> {
    let certificate = crate::x509::certificate_identity(cert_pem)
        .map_err(|e| format!("the certificate cannot be read: {e}"))?;
    if &certificate.ed25519_public_key != public_key {
        return Err("the certificate's public key is not the agent's key".to_string());
    }
    if certificate.common_name != agent_id {
        return Err("the certificate's common name is not the agent's id".to_string());
    }
    Ok(())
}

/// Harden a persisted artifact to owner-only access (NFR-003).
///
/// Windows: replace the file's DACL with a protected one (no inherited
/// entries) holding exactly two entries, full control for the current
/// user's SID and for `SYSTEM` (S-1-5-18). Any entry the file was created
/// with, inherited or explicit, is gone afterwards.
#[cfg(windows)]
fn harden(path: &Path) -> Result<(), EnrollmentError> {
    win_acl::set_owner_only_dacl(path).map_err(|e| {
        EnrollmentError::Persistence(format!("cannot restrict {}: {e}", path.display()))
    })
}

/// The Win32 calls behind `harden`: the current user's SID from the
/// process token, the well-known SYSTEM SID, and a protected DACL set
/// with `SetNamedSecurityInfoW`.
#[cfg(windows)]
mod win_acl {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
    use windows_sys::Win32::Security::Authorization::{
        SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W, NO_MULTIPLE_TRUSTEE,
        SET_ACCESS, SE_FILE_OBJECT, TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN, TRUSTEE_W,
    };
    use windows_sys::Win32::Security::{
        CreateWellKnownSid, GetTokenInformation, TokenUser, WinLocalSystemSid, ACL,
        DACL_SECURITY_INFORMATION, NO_INHERITANCE, PROTECTED_DACL_SECURITY_INFORMATION, PSID,
        SECURITY_MAX_SID_SIZE, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    pub(super) fn set_owner_only_dacl(path: &Path) -> Result<(), String> {
        // TOKEN_USER holds a pointer into this buffer: keep it u64-aligned.
        let token_user = current_token_user()?;
        // SAFETY: `token_user` holds a TOKEN_USER written by
        // GetTokenInformation(TokenUser); its Sid points into the buffer.
        let user_sid: PSID = unsafe { (*(token_user.as_ptr() as *const TOKEN_USER)).User.Sid };

        let mut system_sid = [0u32; (SECURITY_MAX_SID_SIZE as usize).div_ceil(4)];
        let mut system_sid_len = SECURITY_MAX_SID_SIZE;
        // SAFETY: the buffer holds SECURITY_MAX_SID_SIZE bytes, the size passed.
        let ok = unsafe {
            CreateWellKnownSid(
                WinLocalSystemSid,
                std::ptr::null_mut(),
                system_sid.as_mut_ptr().cast(),
                &mut system_sid_len,
            )
        };
        if ok == 0 {
            return Err(format!(
                "CreateWellKnownSid: {}",
                std::io::Error::last_os_error()
            ));
        }

        let entries = [
            full_control(user_sid),
            full_control(system_sid.as_mut_ptr().cast()),
        ];
        let mut acl: *mut ACL = std::ptr::null_mut();
        // SAFETY: two initialized entries whose SIDs outlive the call; no
        // old ACL; `acl` receives a LocalAlloc'd ACL freed below.
        let rc = unsafe { SetEntriesInAclW(2, entries.as_ptr(), std::ptr::null(), &mut acl) };
        if rc != 0 {
            return Err(format!(
                "SetEntriesInAclW: {}",
                std::io::Error::from_raw_os_error(rc as i32)
            ));
        }

        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: `wide` is NUL-terminated; `acl` is a valid ACL; the
        // owner, group and SACL are left unchanged (null).
        let rc = unsafe {
            SetNamedSecurityInfoW(
                wide.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                acl,
                std::ptr::null(),
            )
        };
        // SAFETY: `acl` was allocated by SetEntriesInAclW.
        unsafe { LocalFree(acl.cast()) };
        if rc != 0 {
            return Err(format!(
                "SetNamedSecurityInfoW: {}",
                std::io::Error::from_raw_os_error(rc as i32)
            ));
        }
        Ok(())
    }

    /// The process token's TOKEN_USER, in a u64-aligned buffer.
    fn current_token_user() -> Result<Vec<u64>, String> {
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: the pseudo-handle of the current process needs no close;
        // `token` receives a handle closed below.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(format!(
                "OpenProcessToken: {}",
                std::io::Error::last_os_error()
            ));
        }
        let mut needed = 0u32;
        // SAFETY: a size query (null buffer, length 0) on a valid token.
        unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed) };
        let mut buffer = vec![0u64; (needed as usize).div_ceil(8).max(1)];
        let mut written = 0u32;
        // SAFETY: `buffer` holds at least `needed` bytes.
        let ok = unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 8) as u32,
                &mut written,
            )
        };
        let error = std::io::Error::last_os_error();
        // SAFETY: `token` was opened above.
        unsafe { CloseHandle(token) };
        if ok == 0 {
            return Err(format!("GetTokenInformation(TokenUser): {error}"));
        }
        Ok(buffer)
    }

    fn full_control(sid: PSID) -> EXPLICIT_ACCESS_W {
        EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_ALL_ACCESS,
            grfAccessMode: SET_ACCESS,
            grfInheritance: NO_INHERITANCE,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: std::ptr::null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_UNKNOWN,
                ptstrName: sid.cast(),
            },
        }
    }
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

/// The installed service's identity (SPEC-024 §Data contracts §2,
/// §Operational §2 and its Amendment 2026-11-04, items 3 to 6). The console
/// keeps the functions above.
#[cfg(windows)]
pub mod installed {
    use super::{check_consistency, is_enrolled, load_identity, without_token, Identity};
    use super::{Path, PersistedIdentity};
    use crate::config::AgentConfig;
    use crate::crypto::{pubkey_fingerprint, AgentKeypair};
    use crate::custody::{self, Kind, Policy};
    use crate::errors::EnrollmentError;
    use crate::secure_storage::default_store;
    use crate::service::InstalledPaths;
    use std::path::PathBuf;

    /// `identity\`: owner SYSTEM, SYSTEM alone, inherited by its files.
    pub const DIRECTORY_SDDL: &str = "O:SYD:P(A;OICI;FA;;;SY)";
    /// Each identity file: owner SYSTEM, SYSTEM alone.
    pub const FILE_SDDL: &str = "O:SYD:P(A;;FA;;;SY)";

    /// Where the identity lives, what its custody check accepts and the
    /// security what is written gets, and what `agent.toml`'s check
    /// accepts. Tests point it at a temporary root.
    #[derive(Debug, Clone)]
    pub struct InstalledCustody {
        pub identity_dir: PathBuf,
        pub hygiene_temporary: PathBuf,
        pub policy: Policy,
        pub config_policy: Policy,
        pub directory_sddl: String,
        pub file_sddl: String,
    }

    impl InstalledCustody {
        /// The service's: SYSTEM alone, at the fixed paths.
        pub fn system(paths: &InstalledPaths) -> Self {
            InstalledCustody {
                identity_dir: paths.identity_dir.clone(),
                hygiene_temporary: paths.hygiene_temporary(),
                policy: Policy::system_only(),
                config_policy: Policy::configuration(),
                directory_sddl: DIRECTORY_SDDL.to_string(),
                file_sddl: FILE_SDDL.to_string(),
            }
        }
    }

    /// Item 4's repair: `identity\` holds the set, so removing it is what
    /// lets the service enroll again.
    const REPAIR: &str =
        "restore the identity, or remove the identity directory and enroll with a new token";

    fn persistence(reason: String) -> EnrollmentError {
        EnrollmentError::Persistence(reason)
    }

    /// The identity step's check (item 3), before any identity file is read
    /// and before enrollment: `identity\` and each of its files that exists
    /// pass the custody check, then the set is complete or absent (SPEC-002
    /// Amendment 2026-10-27), absent only when `identity\` is (item 4): an
    /// `identity\` without all three files, none included, is incomplete.
    /// Code 5.
    pub fn check(config: &AgentConfig, custody: &InstalledCustody) -> Result<(), EnrollmentError> {
        let enr = config
            .enrollment
            .as_ref()
            .ok_or(EnrollmentError::MissingToken)?;
        // Item 4: the set is absent only when `identity\` is.
        if !custody::check_if_exists(&custody.identity_dir, Kind::Directory, &custody.policy)
            .map_err(persistence)?
        {
            return Ok(());
        }
        let mut missing = Vec::new();
        for (name, file) in [
            ("the certificate", &enr.cert_path),
            ("the key", &enr.key_path),
            ("identity.json", &enr.identity_path),
        ] {
            if !custody::check_if_exists(Path::new(file), Kind::File, &custody.policy)
                .map_err(persistence)?
            {
                missing.push(name);
            }
        }
        if missing.is_empty() {
            return Ok(());
        }
        Err(persistence(format!(
            "incomplete identity: {} missing; {REPAIR}",
            missing.join(" and ")
        )))
    }

    /// The dispatcher of `super::ensure_identity`, in installed mode: the
    /// loaded set is checked for consistency, an issued certificate before
    /// it is written, and the identity and the token's rewrite keep their
    /// custody. `check` runs first.
    pub async fn ensure_identity(
        config: &AgentConfig,
        config_path: &Path,
        custody: &InstalledCustody,
    ) -> Result<Identity, EnrollmentError> {
        let enr = config
            .enrollment
            .as_ref()
            .ok_or(EnrollmentError::MissingToken)?;
        if is_enrolled(config) {
            if enr.token.as_deref().is_some_and(|t| !t.is_empty()) {
                tracing::warn!("stale enrollment token present in config, ignoring");
            }
            let identity = load(config)?;
            tracing::info!(agent_id = %identity.agent_id, "identity loaded from disk");
            return Ok(identity);
        }
        let enrolled = crate::enrollment::enroll(config).await?;
        check_issued(&enrolled)?;
        persist(config, &enrolled, custody)?;
        tracing::info!(
            cert_path = %enr.cert_path,
            key_path = %enr.key_path,
            identity_path = %enr.identity_path,
            "identity persisted"
        );
        hygienize_token(config_path, custody);
        let keypair = AgentKeypair::from_secret_bytes(&enrolled.secret_seed);
        Ok(Identity {
            agent_id: enrolled.agent_id,
            keypair,
            client_certificate_pem: enrolled.client_certificate_pem,
        })
    }

    /// The server's answer names a certificate for this agent's key and id;
    /// otherwise it is malformed, code 3, and nothing is written (item 5).
    pub fn check_issued(
        enrolled: &crate::enrollment::EnrolledIdentity,
    ) -> Result<(), EnrollmentError> {
        check_consistency(
            &enrolled.client_certificate_pem,
            &enrolled.public_key,
            &enrolled.agent_id,
        )
        .map_err(|e| EnrollmentError::Refused(format!("malformed response: {e}")))
    }

    /// FR-008's load, then item 5's consistency. Code 5, naming the repair.
    pub fn load(config: &AgentConfig) -> Result<Identity, EnrollmentError> {
        let identity = load_identity(config).map_err(|e| match e {
            EnrollmentError::Persistence(reason) => persistence(format!("{reason}; {REPAIR}")),
            other => other,
        })?;
        check_consistency(
            &identity.client_certificate_pem,
            &identity.keypair.public_key_bytes(),
            &identity.agent_id,
        )
        .map_err(|e| persistence(format!("inconsistent identity: {e}; {REPAIR}")))?;
        Ok(identity)
    }

    /// Item 4: `identity\` with its final security when absent, then
    /// `cert.pem`, `key.dat` and `identity.json`, each created new with its
    /// final security and flushed; `identity.json` last.
    pub fn persist(
        config: &AgentConfig,
        enrolled: &crate::enrollment::EnrolledIdentity,
        custody: &InstalledCustody,
    ) -> Result<(), EnrollmentError> {
        let enr = config
            .enrollment
            .as_ref()
            .ok_or(EnrollmentError::MissingToken)?;
        custody::ensure_directory(
            &custody.identity_dir,
            &custody.directory_sddl,
            &custody.policy,
        )
        .map_err(persistence)?;
        let sealed = default_store()
            .seal(&enrolled.secret_seed[..])
            .map_err(|e| persistence(format!("cannot seal private key: {e}")))?;
        let persisted = PersistedIdentity {
            agent_id: enrolled.agent_id.clone(),
            agent_pubkey_fingerprint: pubkey_fingerprint(&enrolled.public_key),
            issued_at: enrolled.issued_at.clone(),
            expires_at: enrolled.expires_at.clone(),
        };
        let identity_json = serde_json::to_string_pretty(&persisted)
            .map_err(|e| persistence(format!("cannot serialize identity.json: {e}")))?;
        for (path, bytes) in [
            (&enr.cert_path, enrolled.client_certificate_pem.as_bytes()),
            (&enr.key_path, sealed.as_slice()),
            (&enr.identity_path, identity_json.as_bytes()),
        ] {
            custody::create_new_file(Path::new(path), &custody.file_sddl, bytes)
                .map_err(persistence)?;
        }
        Ok(())
    }

    /// FR-014 in installed mode (item 6): the rewrite keeps the original's
    /// owner and DACL. Best-effort, as the console's.
    pub fn hygienize_token(config_path: &Path, custody: &InstalledCustody) {
        // A TOML error can quote a line of the file, which holds the token:
        // the warning names the failure, not the parser's message.
        let rewritten = std::fs::read_to_string(config_path)
            .map_err(|e| e.to_string())
            .and_then(|content| {
                without_token(&content).map_err(|_| "agent.toml is not valid TOML".to_string())
            })
            .and_then(|text| {
                custody::replace_keeping_security(
                    config_path,
                    &custody.hygiene_temporary,
                    text.as_bytes(),
                    &custody.config_policy,
                )
            });
        if let Err(e) = rewritten {
            tracing::warn!(error = %e, "could not hygienize enrollment token from config");
        }
    }
}
