//! The setup's inputs (SPEC-024 §Data contracts §3 and its Amendment
//! 2026-11-09, item 2): the bootstrap file's keys, and their validation, which
//! the wizard's input will share.

use std::collections::BTreeSet;
use std::path::PathBuf;

use cg_agent::service::anchor::is_fingerprint;
use zeroize::{Zeroize, Zeroizing};

/// The keys a bootstrap file admits, and requires: any other is refused.
pub const ADMITTED: [&str; 5] = [
    "server.url",
    "server.heartbeat_url",
    "server.trust_anchor_path",
    "enrollment.token",
    "setup.trust_anchor_sha256",
];

/// The keys of the agent's configuration (SPEC-001, SPEC-002, SPEC-003) and
/// the setup's own: a refusal names a key it does not admit only when it is
/// one of these, so that no line repeats a key the operator wrote, which may
/// be a token in the wrong place.
const KNOWN: &[&str] = &[
    "agent",
    "agent.hostname",
    "agent.id",
    "enrollment",
    "enrollment.backoff_factor",
    "enrollment.backoff_initial_ms",
    "enrollment.cert_path",
    "enrollment.identity_path",
    "enrollment.key_path",
    "enrollment.max_retries",
    "enrollment.timeout_seconds",
    "enrollment.token",
    "envelope",
    "envelope.canonical_form",
    "heartbeat",
    "heartbeat.backoff_factor",
    "heartbeat.backoff_initial_ms",
    "heartbeat.backoff_max_ms",
    "heartbeat.interval_seconds",
    "heartbeat.max_retries",
    "heartbeat.request_timeout_seconds",
    "log",
    "log.level",
    "server",
    "server.heartbeat_url",
    "server.trust_anchor_path",
    "server.url",
    "setup",
    "setup.trust_anchor_sha256",
    "tls",
    "tls.minimum_version",
];

/// What the setup is given. Its `Debug` writes no token.
pub struct Inputs {
    pub server_url: String,
    pub heartbeat_url: String,
    /// `server.trust_anchor_path`: where the anchor is read from, once.
    pub anchor_source: PathBuf,
    token: Zeroizing<String>,
    /// `setup.trust_anchor_sha256`, its duplicates collapsed.
    pub fingerprints: BTreeSet<String>,
}

impl Inputs {
    /// The enrollment token.
    pub fn token(&self) -> &str {
        &self.token
    }
}

impl std::fmt::Debug for Inputs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inputs")
            .field("server_url", &self.server_url)
            .field("heartbeat_url", &self.heartbeat_url)
            .field("anchor_source", &self.anchor_source)
            .field("token", &"<redacted>")
            .field("fingerprints", &self.fingerprints)
            .finish()
    }
}

/// The parser's 1-based line and column of `error`, without its message,
/// which quotes the line.
fn position(text: &str, error: &toml::de::Error) -> String {
    match error.span() {
        Some(span) => {
            let before = text.get(..span.start).unwrap_or(text);
            let line = before.matches('\n').count() + 1;
            let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
            format!("the file is not valid TOML: line {line}, column {column}")
        }
        None => "the file is not valid TOML".to_string(),
    }
}

/// A key the file holds and does not admit, named only when it is known.
fn not_admitted(table: Option<&str>, key: &str) -> String {
    let full = table.map_or_else(|| key.to_string(), |table| format!("{table}.{key}"));
    if KNOWN.contains(&full.as_str()) {
        let supplied = if full.starts_with("agent")
            || matches!(
                full.as_str(),
                "enrollment.cert_path" | "enrollment.key_path" | "enrollment.identity_path"
            ) {
            ": the setup supplies it"
        } else {
            ""
        };
        format!("{full} is not a key the bootstrap file admits{supplied}")
    } else {
        match table {
            Some(table) => format!("a key of [{table}] that is not admitted"),
            None => "a top-level key that is not admitted".to_string(),
        }
    }
}

/// Refuse a document that holds any key it does not admit.
fn check_keys(document: &toml::Table) -> Result<(), String> {
    for (name, value) in document {
        let admitted: Vec<&str> = ADMITTED
            .iter()
            .filter_map(|key| key.strip_prefix(&format!("{name}.")))
            .collect();
        if admitted.is_empty() {
            return Err(not_admitted(None, name));
        }
        let Some(table) = value.as_table() else {
            return Err(format!("{name} must be a table"));
        };
        if let Some(key) = table.keys().find(|key| !admitted.contains(&key.as_str())) {
            return Err(not_admitted(Some(name), key));
        }
    }
    Ok(())
}

/// The string at `table.key`, taken out of the document.
fn take_string(document: &mut toml::Table, table: &str, key: &str) -> Result<String, String> {
    let full = format!("{table}.{key}");
    match document
        .get_mut(table)
        .and_then(|t| t.as_table_mut())
        .and_then(|t| t.remove(key))
    {
        None => Err(format!("{full} is required")),
        Some(toml::Value::String(value)) => Ok(value),
        Some(_) => Err(format!("{full} must be a string")),
    }
}

/// The bootstrap file's text, validated: the error is the reason, which
/// names a key or a position, never a value or a line of the file.
pub fn parse_bootstrap(text: &str) -> Result<Inputs, String> {
    let document: toml::Table = toml::from_str(text).map_err(|e| position(text, &e))?;
    parse_table(document)
}

/// A document of the bootstrap file's keys, validated as the file's is: the
/// wizard's fields pass through here too (SPEC-024 Amendment 2026-11-13,
/// point 4). The error names a key, never a value.
pub(crate) fn parse_table(mut document: toml::Table) -> Result<Inputs, String> {
    let parsed = parse_document(&mut document);
    // The token, if it is still there, and any other string go with the
    // document; the token taken out is zeroed by its own wrapper.
    if let Some(toml::Value::String(token)) = document
        .get_mut("enrollment")
        .and_then(|t| t.as_table_mut())
        .and_then(|t| t.get_mut("token"))
    {
        token.zeroize();
    }
    parsed
}

fn parse_document(document: &mut toml::Table) -> Result<Inputs, String> {
    check_keys(document)?;
    let server_url = take_string(document, "server", "url")?;
    let heartbeat_url = take_string(document, "server", "heartbeat_url")?;
    let anchor_source = take_string(document, "server", "trust_anchor_path")?;
    let token = Zeroizing::new(take_string(document, "enrollment", "token")?);
    if token.is_empty() {
        return Err("enrollment.token must not be empty".to_string());
    }
    let fingerprints = match document
        .get("setup")
        .and_then(|t| t.get("trust_anchor_sha256"))
    {
        None => return Err("setup.trust_anchor_sha256 is required".to_string()),
        Some(toml::Value::Array(values)) => {
            let mut fingerprints = BTreeSet::new();
            for value in values {
                match value.as_str() {
                    Some(text) if is_fingerprint(text) => {
                        fingerprints.insert(text.to_string());
                    }
                    _ => {
                        return Err("setup.trust_anchor_sha256 holds a value that is not \
                             sha256: and 64 lowercase hexadecimal digits"
                            .to_string())
                    }
                }
            }
            if fingerprints.is_empty() {
                return Err("setup.trust_anchor_sha256 must not be empty".to_string());
            }
            fingerprints
        }
        Some(_) => return Err("setup.trust_anchor_sha256 must be an array".to_string()),
    };
    Ok(Inputs {
        server_url,
        heartbeat_url,
        anchor_source: PathBuf::from(anchor_source),
        token,
        fingerprints,
    })
}
