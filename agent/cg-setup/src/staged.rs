//! What the setup stages (SPEC-024 Amendment 2026-11-09, item 3, and §Data
//! contracts §2 and §3): the configuration, `agent.toml.setup`, and the trust
//! anchor, `server-ca.pem.setup`, as the service's adoption takes them in
//! (item 6.4).

use std::path::Path;

use cg_agent::service::InstalledPaths;
use toml::{Table, Value};
use zeroize::{Zeroize, Zeroizing};

use crate::inputs::Inputs;

/// A placeholder `agent.id`, a new UUIDv7: never a claimed enrolled identity
/// (§Data contracts §2).
pub fn placeholder_id() -> String {
    uuid::Uuid::now_v7().hyphenated().to_string()
}

fn path_text(path: &Path) -> String {
    // The fixed paths come from the OS and `InstalledPaths`; the service
    // compares them as text, so a path that is not Unicode could never pass.
    path.to_string_lossy().into_owned()
}

/// The staged configuration: the two URLs and the token of `inputs`, and
/// what the setup supplies, the fixed installed paths, `hostname` and
/// `agent_id`. Every other field takes its default; there is no `setup`
/// table.
pub fn staged_config(
    paths: &InstalledPaths,
    inputs: &Inputs,
    agent_id: &str,
    hostname: &str,
) -> Zeroizing<String> {
    let string = |text: &str| Value::String(text.to_string());
    let mut server = Table::new();
    server.insert("url".into(), string(&inputs.server_url));
    server.insert("heartbeat_url".into(), string(&inputs.heartbeat_url));
    server.insert(
        "trust_anchor_path".into(),
        string(&path_text(&paths.trust_anchor)),
    );
    let mut agent = Table::new();
    agent.insert("id".into(), string(agent_id));
    agent.insert("hostname".into(), string(hostname));
    let mut enrollment = Table::new();
    enrollment.insert("token".into(), string(inputs.token()));
    enrollment.insert("cert_path".into(), string(&path_text(&paths.cert)));
    enrollment.insert("key_path".into(), string(&path_text(&paths.key)));
    enrollment.insert("identity_path".into(), string(&path_text(&paths.identity)));
    let mut document = Table::new();
    document.insert("server".into(), Value::Table(server));
    document.insert("agent".into(), Value::Table(agent));
    document.insert("enrollment".into(), Value::Table(enrollment));

    // A table of strings always serializes.
    let text = Zeroizing::new(toml::to_string(&document).expect("a table of strings"));
    if let Some(Value::String(token)) = document
        .get_mut("enrollment")
        .and_then(|t| t.as_table_mut())
        .and_then(|t| t.get_mut("token"))
    {
        token.zeroize();
    }
    text
}

/// The staged trust anchor: the certificates checked against the
/// fingerprints, in their order, as PEM, and nothing else of the source.
pub fn staged_anchor(certificates: &[Vec<u8>]) -> Vec<u8> {
    let blocks: Vec<pem::Pem> = certificates
        .iter()
        .map(|der| pem::Pem::new("CERTIFICATE", der.clone()))
        .collect();
    pem::encode_many_config(
        &blocks,
        pem::EncodeConfig::new().set_line_ending(pem::LineEnding::LF),
    )
    .into_bytes()
}

/// The adoption's checks of the staged configuration, applied to `text`
/// (item 3: "by the same checks the service applies"); the error is the
/// service's line.
pub fn check_staged(paths: &InstalledPaths, text: &str) -> Result<(), String> {
    cg_agent::service::check_staged_config(paths, &paths.staged_config(), text.as_bytes())
}
