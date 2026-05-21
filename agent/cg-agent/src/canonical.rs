//! JSON Canonicalization Scheme (RFC 8785, JCS) for the signed region of
//! the outer envelope. See SPEC-003 §Data contracts and §Security
//! considerations.
//!
//! The bytes the agent signs and the bytes the server verifies must be
//! byte-identical, so both sides run the same canonicalization. The
//! envelope's value space is restricted to ASCII strings, unsigned
//! integers, and nested objects (no floats, no sub-millisecond
//! fractions), so the JCS number-formatting corners never arise.

use crate::errors::SigningError;

/// Canonicalize a JSON value to its RFC 8785 (JCS) byte form.
pub fn canonical_bytes(_value: &serde_json::Value) -> Result<Vec<u8>, SigningError> {
    todo!("implemented in the SPEC-003 implementation commit")
}
