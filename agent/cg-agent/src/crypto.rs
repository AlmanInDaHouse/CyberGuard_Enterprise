//! Ed25519 key material for the agent identity. See SPEC-002 §FR-003
//! and §Security considerations.
//!
//! The private key never leaves a `zeroize`-protected wrapper except
//! transiently inside `ed25519_dalek::SigningKey`, which itself zeroes
//! its secret on drop (the crate's `zeroize` feature is enabled).

use sha2::{Digest, Sha256};

/// Length in bytes of an Ed25519 secret seed and of the public key.
pub const KEY_LEN: usize = 32;

/// An agent keypair: the signing key (private) plus its derived public
/// key bytes. The signing key zeroes itself on drop.
pub struct AgentKeypair {
    signing_key: ed25519_dalek::SigningKey,
    public_key: [u8; KEY_LEN],
}

impl AgentKeypair {
    /// Generate a fresh keypair from OS entropy (`getrandom` →
    /// BCryptGenRandom on Windows, /dev/urandom on Unix).
    pub fn generate() -> Result<Self, getrandom::Error> {
        todo!("implemented in the enrollment commit")
    }

    /// Reconstruct a keypair from a 32-byte secret seed (e.g. after
    /// unsealing `key.dat`).
    pub fn from_secret_bytes(_secret: &[u8; KEY_LEN]) -> Self {
        todo!("implemented in the enrollment commit")
    }

    /// The raw 32-byte public key.
    pub fn public_key_bytes(&self) -> [u8; KEY_LEN] {
        self.public_key
    }

    /// Borrow the underlying signing key (for future SPEC-003 signing).
    pub fn signing_key(&self) -> &ed25519_dalek::SigningKey {
        &self.signing_key
    }

    /// Expose the secret seed bytes for sealing. Returned inside a
    /// `Zeroizing` wrapper so the copy is wiped on drop.
    pub fn secret_seed(&self) -> zeroize::Zeroizing<[u8; KEY_LEN]> {
        zeroize::Zeroizing::new(self.signing_key.to_bytes())
    }
}

/// Manual `Debug` that prints only the public-key fingerprint — the
/// secret seed is never rendered (NFR-002: private-key material must not
/// leak into logs or panic output). `finish_non_exhaustive` marks the
/// elided secret field with `..`.
impl std::fmt::Debug for AgentKeypair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentKeypair")
            .field(
                "public_key_fingerprint",
                &pubkey_fingerprint(&self.public_key),
            )
            .finish_non_exhaustive()
    }
}

/// SHA-256 fingerprint (lowercase hex) of a 32-byte public key. Used as
/// the tamper-evidence cross-check in `identity.json` (SPEC-002 §FR-008).
pub fn pubkey_fingerprint(public_key: &[u8; KEY_LEN]) -> String {
    let digest = Sha256::digest(public_key);
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}
