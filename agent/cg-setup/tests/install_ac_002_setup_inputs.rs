//! install_ac_002's setup half for the inputs, on every platform (SPEC-024
//! §Data contracts §3 and its Amendment 2026-11-09, items 2 and 3): the
//! bootstrap file admits exactly its five keys, each in its form; the setup
//! supplies the fixed paths, the host name and the placeholder ID; the
//! configuration it generates passes the checks the service's adoption
//! applies, which refuse plain HTTP; and no reason carries the token.

mod fixtures;

use std::path::Path;

use cg_agent::service::anchor::check_anchor;
use cg_agent::service::InstalledPaths;
use cg_setup::inputs::{parse_bootstrap, ADMITTED};
use cg_setup::staged::{check_staged, placeholder_id, staged_anchor, staged_config};

use fixtures::{
    assert_clean, bootstrap_text, certificate, quoted, set, ENROLL_URL, HEARTBEAT_URL, HOSTNAME,
    TOKEN,
};

const SOURCE: &str = "C:\\operator\\server-ca.pem";

fn a_fingerprint(n: u8) -> String {
    format!("sha256:{}", format!("{n:02x}").repeat(32))
}

fn valid() -> String {
    bootstrap_text(SOURCE, &[&a_fingerprint(1)])
}

fn refused(text: &str, naming: &str) {
    match parse_bootstrap(text) {
        Ok(inputs) => panic!("{naming}: refused was expected: {inputs:?}"),
        Err(reason) => {
            assert!(reason.contains(naming), "{naming}: {reason}");
            assert_clean(&reason);
        }
    }
}

fn program_data() -> &'static Path {
    if cfg!(windows) {
        Path::new("C:\\ProgramData")
    } else {
        Path::new("/ProgramData")
    }
}

// --- The bootstrap file's keys ----------------------------------------------

#[test]
fn install_ac_002_setup_a_bootstrap_with_its_five_keys_is_taken() {
    let fingerprint = a_fingerprint(1);
    let inputs = parse_bootstrap(&valid()).expect("the five keys");
    assert_eq!(inputs.server_url, ENROLL_URL);
    assert_eq!(inputs.heartbeat_url, HEARTBEAT_URL);
    assert_eq!(inputs.anchor_source, Path::new(SOURCE));
    assert_eq!(inputs.token(), TOKEN);
    assert_eq!(inputs.fingerprints, set(&[&fingerprint]));
    assert_eq!(
        ADMITTED,
        [
            "server.url",
            "server.heartbeat_url",
            "server.trust_anchor_path",
            "enrollment.token",
            "setup.trust_anchor_sha256",
        ]
    );
}

#[test]
fn install_ac_002_setup_any_other_key_is_refused() {
    let base = valid();
    for (extra, naming) in [
        (
            "[agent]\nid = \"01934abc-def0-7000-89ab-0000000000bb\"\n",
            "agent",
        ),
        ("[agent]\nhostname = \"other\"\n", "agent"),
        ("[log]\nlevel = \"debug\"\n", "log"),
        ("[heartbeat]\ninterval_seconds = 5\n", "heartbeat"),
        ("[tls]\nminimum_version = \"1.3\"\n", "tls"),
        ("unrelated = 1\n", "a top-level key that is not admitted"),
    ] {
        refused(&format!("{extra}{base}"), naming);
    }
    // Keys inside the admitted tables: the fixed paths are the setup's.
    for (table, line, naming) in [
        (
            "server",
            "plain = true",
            "a key of [server] that is not admitted",
        ),
        (
            "enrollment",
            "cert_path = \"C:\\\\x\\\\cert.pem\"",
            "enrollment.cert_path",
        ),
        (
            "enrollment",
            "key_path = \"C:\\\\x\\\\key.dat\"",
            "enrollment.key_path",
        ),
        (
            "enrollment",
            "identity_path = \"C:\\\\x\\\\identity.json\"",
            "enrollment.identity_path",
        ),
        ("enrollment", "max_retries = 9", "enrollment.max_retries"),
        (
            "setup",
            "other = 1",
            "a key of [setup] that is not admitted",
        ),
    ] {
        let text = base.replacen(&format!("[{table}]\n"), &format!("[{table}]\n{line}\n"), 1);
        assert_ne!(text, base, "{naming}: the line was added");
        refused(&text, naming);
    }
}

#[test]
fn install_ac_002_setup_each_admitted_key_is_required() {
    let base = valid();
    for (key, line_start) in [
        ("server.url", "url = "),
        ("server.heartbeat_url", "heartbeat_url = "),
        ("server.trust_anchor_path", "trust_anchor_path = "),
        ("enrollment.token", "token = "),
        ("setup.trust_anchor_sha256", "trust_anchor_sha256 = "),
    ] {
        let text: String = base
            .lines()
            .filter(|line| !line.starts_with(line_start))
            .map(|line| format!("{line}\n"))
            .collect();
        assert_ne!(text, base, "{key}: the line was removed");
        refused(&text, key);
    }
}

#[test]
fn install_ac_002_setup_each_key_has_its_form() {
    let base = valid();
    let with = |from: &str, to: &str| {
        assert!(base.contains(from), "{from}");
        base.replacen(from, to, 1)
    };
    let url = format!("url = {}", quoted(ENROLL_URL));
    let token = format!("token = {}", quoted(TOKEN));
    let fingerprints = format!("trust_anchor_sha256 = [{}]", quoted(&a_fingerprint(1)));
    for (text, naming) in [
        (with(&url, "url = 8444"), "server.url"),
        (with(&token, "token = 7"), "enrollment.token"),
        (with(&token, "token = \"\""), "enrollment.token"),
        (
            with(&fingerprints, "trust_anchor_sha256 = []"),
            "setup.trust_anchor_sha256",
        ),
        (
            with(
                &fingerprints,
                &format!("trust_anchor_sha256 = {}", quoted(&a_fingerprint(1))),
            ),
            "setup.trust_anchor_sha256",
        ),
        (
            with(&fingerprints, "trust_anchor_sha256 = [1]"),
            "setup.trust_anchor_sha256",
        ),
        (
            format!(
                "server = {}\n{}",
                quoted(ENROLL_URL),
                &base[base.find("[enrollment]").expect("enrollment")..]
            ),
            "server",
        ),
        (
            format!("{base}\n[server.extra]\nx = 1\n"),
            "a key of [server] that is not admitted",
        ),
    ] {
        refused(&text, naming);
    }
    for bad in [
        a_fingerprint(1).to_uppercase(),
        a_fingerprint(1).replacen("sha256:", "sha1:", 1),
        a_fingerprint(1).replacen("sha256:", "", 1),
        a_fingerprint(1)[..70].to_string(),
        format!("{}0", a_fingerprint(1)),
        a_fingerprint(1).replacen('0', "g", 1),
    ] {
        refused(
            &with(
                &fingerprints,
                &format!("trust_anchor_sha256 = [{}]", quoted(&bad)),
            ),
            "setup.trust_anchor_sha256",
        );
    }
}

#[test]
fn install_ac_002_setup_repeated_fingerprints_are_one() {
    let (one, two) = (a_fingerprint(1), a_fingerprint(2));
    let inputs = parse_bootstrap(&bootstrap_text(SOURCE, &[&one, &two, &one])).expect("repeated");
    assert_eq!(inputs.fingerprints, set(&[&one, &two]));
}

#[test]
fn install_ac_002_setup_a_malformed_file_is_refused_without_quoting_it() {
    let base = valid();
    // The token's line broken: the parser's own message would quote it.
    let unterminated = base.replacen(
        &format!("token = {}", quoted(TOKEN)),
        &format!("token = \"{TOKEN}"),
        1,
    );
    let reason = parse_bootstrap(&unterminated).expect_err("malformed");
    assert!(reason.contains("line 7"), "{reason}");
    assert_clean(&reason);
    for text in [
        "[server\n".to_string(),
        format!("{base}\n[enrollment]\ntoken = {}\n", quoted(TOKEN)),
        format!("{TOKEN} = 1\n{base}"),
    ] {
        let reason = parse_bootstrap(&text).expect_err("malformed or repeated");
        assert_clean(&reason);
    }
}

#[test]
fn install_ac_002_setup_inputs_print_no_token() {
    let inputs = parse_bootstrap(&valid()).expect("valid");
    let debug = format!("{inputs:?}");
    assert_clean(&debug);
    assert!(debug.contains(ENROLL_URL), "{debug}");
}

// --- The staged configuration -------------------------------------------------

#[test]
fn install_ac_002_setup_the_staged_configuration_is_the_inputs_and_the_fixed_paths() {
    let paths = InstalledPaths::under(program_data());
    let inputs = parse_bootstrap(&valid()).expect("valid");
    let id = placeholder_id();
    let text = staged_config(&paths, &inputs, &id, HOSTNAME);
    check_staged(&paths, &text).expect("the adoption's checks");
    cg_agent::service::check_staged_config(&paths, &paths.staged_config(), text.as_bytes())
        .expect("the service's own call");

    let document: toml::Value = text.parse().expect("TOML");
    let string = |table: &str, key: &str| {
        document[table][key]
            .as_str()
            .unwrap_or_else(|| panic!("{table}.{key}"))
            .to_string()
    };
    let text_of = |path: &Path| path.to_str().expect("UTF-8").to_string();
    assert_eq!(string("server", "url"), ENROLL_URL);
    assert_eq!(string("server", "heartbeat_url"), HEARTBEAT_URL);
    assert_eq!(
        string("server", "trust_anchor_path"),
        text_of(&paths.trust_anchor)
    );
    assert_eq!(string("agent", "id"), id);
    assert_eq!(string("agent", "hostname"), HOSTNAME);
    assert_eq!(string("enrollment", "token"), TOKEN);
    assert_eq!(string("enrollment", "cert_path"), text_of(&paths.cert));
    assert_eq!(string("enrollment", "key_path"), text_of(&paths.key));
    assert_eq!(
        string("enrollment", "identity_path"),
        text_of(&paths.identity)
    );

    // Nothing else: every other field takes its default, and no setup table.
    let mut keys: Vec<String> = Vec::new();
    for (table, value) in document.as_table().expect("a table") {
        for key in value.as_table().expect("tables alone").keys() {
            keys.push(format!("{table}.{key}"));
        }
    }
    keys.sort();
    assert_eq!(
        keys,
        [
            "agent.hostname",
            "agent.id",
            "enrollment.cert_path",
            "enrollment.identity_path",
            "enrollment.key_path",
            "enrollment.token",
            "server.heartbeat_url",
            "server.trust_anchor_path",
            "server.url",
        ]
    );
}

#[test]
fn install_ac_002_setup_the_placeholder_is_a_new_uuidv7() {
    let (one, two) = (placeholder_id(), placeholder_id());
    assert_ne!(one, two);
    for id in [&one, &two] {
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12],
            "{id}"
        );
        assert!(parts[2].starts_with('7'), "version 7: {id}");
        assert!(
            id.bytes()
                .all(|b| b == b'-' || b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "{id}"
        );
    }
}

#[test]
fn install_ac_002_setup_plain_http_is_refused_by_the_services_checks() {
    let paths = InstalledPaths::under(program_data());
    for (from, to, naming) in [
        (ENROLL_URL, "http://enroll.example.test:8080", "server.url"),
        (
            HEARTBEAT_URL,
            "http://ingest.example.test:8080",
            "server.heartbeat_url",
        ),
    ] {
        let inputs = parse_bootstrap(&valid().replacen(from, to, 1)).expect("the form is valid");
        let text = staged_config(&paths, &inputs, &placeholder_id(), HOSTNAME);
        let line = check_staged(&paths, &text).expect_err("plain HTTP");
        assert!(line.contains(naming) && line.contains("https"), "{line}");
        assert_clean(&line);
    }
}

// --- The staged anchor ----------------------------------------------------------

#[test]
fn install_ac_002_setup_the_staged_anchor_is_the_checked_certificates_alone() {
    let (one, two) = (certificate("one.test"), certificate("two.test"));
    let source = format!(
        "a comment line\n{}{}-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIA==\n-----END PRIVATE KEY-----\n{}",
        one.pem, two.pem, one.pem
    );
    let expected = set(&[&one.fingerprint, &two.fingerprint]);
    let certificates = check_anchor(source.as_bytes(), &expected).expect("the exact set");
    let staged = staged_anchor(&certificates);
    let text = String::from_utf8(staged.clone()).expect("PEM text");
    assert_eq!(text.matches("BEGIN CERTIFICATE").count(), 2, "{text}");
    assert!(
        !text.contains("PRIVATE KEY") && !text.contains("comment"),
        "{text}"
    );
    assert_eq!(
        check_anchor(&staged, &expected).expect("the staged anchor checks"),
        vec![one.der, two.der],
        "the same certificates, in their order"
    );
}

// --- The platform ---------------------------------------------------------------

#[cfg(not(windows))]
#[test]
fn package_preflight_any_other_platform_is_code_20() {
    let refusal = cg_setup::platform::check_platform().expect_err("not Windows");
    assert_eq!(refusal.code, cg_setup::codes::PLATFORM);
    assert!(refusal.line.starts_with("cg-setup: "), "{}", refusal.line);
}
