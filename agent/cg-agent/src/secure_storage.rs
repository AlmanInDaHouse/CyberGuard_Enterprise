//! At-rest secure storage for the agent private key.
//! See SPEC-002 §Security considerations.
//!
//! The Windows implementation uses DPAPI at `CRYPTPROTECT_LOCAL_MACHINE`
//! scope (machine-bound, survives a service-account change). The
//! non-Windows implementation is **test-only** and provides NO
//! confidentiality — it exists so the integration harness can run on
//! Linux CI. A release build for a non-Windows target fails to compile
//! (see the guard below): there is no production secure-storage backend
//! off Windows in the MVP.

// Hard stop: no production secure storage off Windows. debug_assertions
// is on for `cargo test` / dev / CI (all of which run in debug), and
// off for `cargo build --release`. So this only fires on a non-Windows
// RELEASE build, which must not exist until a real backend lands.
#[cfg(all(not(windows), not(debug_assertions)))]
compile_error!(
    "cg-agent has no production secure-storage backend on non-Windows targets. \
     Build for Windows (DPAPI) or implement a real SecureStore (e.g. libsecret) \
     before producing a non-Windows release build."
);

use crate::errors::SecureStoreError;

/// Seals and unseals secret bytes at rest. One concrete impl is selected
/// at compile time per platform; callers use `default_store()`.
pub trait SecureStore {
    /// Encrypt `plaintext` into an at-rest blob (written to `key.dat`).
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, SecureStoreError>;

    /// Decrypt a blob previously produced by `seal` on this machine.
    fn unseal(&self, blob: &[u8]) -> Result<Vec<u8>, SecureStoreError>;

    /// Backend identifier for structured logs. Never includes secrets.
    fn backend_name(&self) -> &'static str;

    /// Whether this backend actually protects data at rest. `false`
    /// for the test-only non-Windows backend; callers log a warning.
    fn is_secure(&self) -> bool;
}

/// Returns the platform default SecureStore.
pub fn default_store() -> Box<dyn SecureStore> {
    #[cfg(windows)]
    {
        Box::new(DpapiSecureStore)
    }
    #[cfg(not(windows))]
    {
        Box::new(InsecureDevStore)
    }
}

// ---------------------------------------------------------------------
// Windows: DPAPI at CRYPTPROTECT_LOCAL_MACHINE scope.
// ---------------------------------------------------------------------

#[cfg(windows)]
pub struct DpapiSecureStore;

#[cfg(windows)]
impl SecureStore for DpapiSecureStore {
    fn seal(&self, _plaintext: &[u8]) -> Result<Vec<u8>, SecureStoreError> {
        todo!("CryptProtectData (CRYPTPROTECT_LOCAL_MACHINE) in the enrollment commit")
    }

    fn unseal(&self, _blob: &[u8]) -> Result<Vec<u8>, SecureStoreError> {
        todo!("CryptUnprotectData in the enrollment commit")
    }

    fn backend_name(&self) -> &'static str {
        "dpapi-local-machine"
    }

    fn is_secure(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------
// Non-Windows: TEST-ONLY plaintext store. NOT secure. Exists so the
// harness runs on Linux CI. Never reachable in a release build (the
// compile_error guard above prevents non-Windows release builds).
// ---------------------------------------------------------------------

#[cfg(not(windows))]
pub struct InsecureDevStore;

#[cfg(not(windows))]
impl SecureStore for InsecureDevStore {
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, SecureStoreError> {
        // No encryption. The blob is the plaintext verbatim, framed with
        // a loud marker so an accidental production artifact is obvious.
        tracing::warn!(
            backend = "insecure-dev-plaintext",
            "SecureStore is the TEST-ONLY non-Windows backend; key material is NOT encrypted at rest"
        );
        let mut framed = b"INSECURE-DEV-PLAINTEXT\n".to_vec();
        framed.extend_from_slice(plaintext);
        Ok(framed)
    }

    fn unseal(&self, blob: &[u8]) -> Result<Vec<u8>, SecureStoreError> {
        tracing::warn!(
            backend = "insecure-dev-plaintext",
            "SecureStore is the TEST-ONLY non-Windows backend; key material was NOT encrypted at rest"
        );
        let marker = b"INSECURE-DEV-PLAINTEXT\n";
        blob.strip_prefix(marker.as_slice())
            .map(|rest| rest.to_vec())
            .ok_or_else(|| SecureStoreError::Unseal("missing dev-store marker".to_string()))
    }

    fn backend_name(&self) -> &'static str {
        "insecure-dev-plaintext"
    }

    fn is_secure(&self) -> bool {
        false
    }
}
