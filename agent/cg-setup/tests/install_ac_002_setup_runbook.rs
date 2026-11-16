//! install_ac_002's setup half for the endpoint runbook, on every platform
//! (SPEC-024 §Data contracts §3; `docs/operations/install.md`, whose endpoint
//! steps run on Windows alone, install_ac_016): the bootstrap file the runbook
//! writes, filled in as PowerShell expands it, is one the setup takes, and its
//! configuration passes the service's checks; and the trust anchor the
//! runbook's server step writes, `export-ca.js`'s output with its trailing
//! `sha256:` line, is read as its certificate alone.

mod fixtures;

use std::path::{Path, PathBuf};

use cg_agent::service::anchor::check_anchor;
use cg_agent::service::InstalledPaths;
use cg_setup::inputs::parse_bootstrap;
use cg_setup::staged::{check_staged, placeholder_id, staged_config};

use fixtures::{certificate, set, HOSTNAME};

const SERVER: &str = "ingest.example.test";
const TOKEN: &str = "runbook-enrollment-token_QUJDREVGR0g-0123456789abcdefghij";

fn runbook() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/operations/install.md");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace('\r', "")
}

/// The endpoint's PowerShell block `id` of the runbook.
fn block(text: &str, id: &str) -> String {
    let marker = format!("<!-- deploy_ac_021: endpoint-{id} -->\n\n```powershell\n");
    let start = text
        .find(&marker)
        .unwrap_or_else(|| panic!("the runbook has no endpoint-{id} block"))
        + marker.len();
    let end = start
        + text[start..]
            .find("\n```\n")
            .unwrap_or_else(|| panic!("endpoint-{id} is not closed"));
    text[start..end].to_string()
}

/// The single-quoted value the block assigns to `$name`.
fn assigned(block: &str, name: &str) -> String {
    let prefix = format!("${name} = '");
    let line = block
        .lines()
        .find(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("no ${name} = '...'"));
    line[prefix.len()..]
        .strip_suffix('\'')
        .unwrap_or_else(|| panic!("${name} is not one quoted value: {line}"))
        .to_string()
}

#[test]
fn install_ac_002_setup_the_runbook_bootstrap_file_is_taken() {
    let text = runbook();
    let dir = assigned(&block(&text, "directory"), "dir");
    let bootstrap = block(&text, "bootstrap");
    let anchor = certificate("ca.runbook.test");

    // The here-string the block writes, expanded as PowerShell expands its
    // variables and its one subexpression, which converts the token read
    // without echo.
    let start = bootstrap.find("@\"\n").expect("a here-string") + 3;
    let end = bootstrap.find("\n\"@").expect("the here-string's end");
    let expanded = bootstrap[start..end]
        .replace("${server}", SERVER)
        .replace("$dir", &dir)
        .replace(
            "$([System.Net.NetworkCredential]::new('', $token).Password)",
            TOKEN,
        )
        .replace("$fingerprint", &anchor.fingerprint);
    assert!(
        !expanded.contains('$') && !expanded.contains('`'),
        "an expansion the test does not make: {expanded}"
    );

    let inputs = parse_bootstrap(&expanded).expect("the setup takes the runbook's file");
    assert_eq!(inputs.server_url, format!("https://{SERVER}:8444"));
    assert_eq!(inputs.heartbeat_url, format!("https://{SERVER}:8443"));
    assert_eq!(inputs.token(), TOKEN);
    assert_eq!(inputs.fingerprints, set(&[&anchor.fingerprint]));
    // The anchor's source is where the runbook copies it.
    let copied = block(&text, "copy");
    assert!(
        copied.contains("\"$dir\\server-ca.pem\""),
        "the runbook copies the anchor into $dir: {copied}"
    );
    // preflight refuses a relative source (24): the runbook's is a drive's absolute path.
    let source = inputs.anchor_source.to_string_lossy().into_owned();
    let bytes = source.as_bytes();
    assert!(
        bytes.len() > 3 && bytes[0].is_ascii_alphabetic() && &bytes[1..3] == b":\\",
        "an absolute Windows path: {source}"
    );
    assert_eq!(
        inputs.anchor_source,
        Path::new(&format!("{dir}\\server-ca.pem"))
    );

    // The configuration the setup generates from it passes the service's checks.
    let program_data = if cfg!(windows) {
        Path::new("C:\\ProgramData")
    } else {
        Path::new("/ProgramData")
    };
    let paths = InstalledPaths::under(program_data);
    let config = staged_config(&paths, &inputs, &placeholder_id(), HOSTNAME);
    check_staged(&paths, &config).expect("the generated configuration is the service's");
}

#[test]
fn install_ac_002_setup_the_anchor_export_ca_writes_is_its_certificate_alone() {
    // services/ingest/src/cli/export-ca.ts: the PEM, ending in a newline, then
    // `sha256:` and the DER's SHA-256 on a line of its own.
    let anchor = certificate("ca.export.test");
    let pem = if anchor.pem.ends_with('\n') {
        anchor.pem.clone()
    } else {
        format!("{}\n", anchor.pem)
    };
    let exported = format!("{pem}{}\n", anchor.fingerprint);
    assert!(exported.ends_with(&format!("\nsha256:{}\n", &anchor.fingerprint[7..])));
    assert_eq!(
        check_anchor(exported.as_bytes(), &set(&[&anchor.fingerprint])).expect("the exact set"),
        vec![anchor.der.clone()],
        "the trailing line is no certificate"
    );
    // The line is no fingerprint either: the list given decides.
    let other = certificate("other.export.test");
    let reason = check_anchor(exported.as_bytes(), &set(&[&other.fingerprint]))
        .expect_err("another fingerprint");
    assert!(
        reason.contains("not exactly those of the fingerprints"),
        "{reason}"
    );
}
