//! SPEC-024 install_ac_017, the names (its Amendment 2026-11-19, point 4):
//! the product's names for each directory and layout are held in one module
//! of `cg-agent`, which the migration, the purge and the reading of the
//! host's layout use, and a name is the product's when its UTF-16 units
//! equal those of a product name or, where case-sensitive lookup is off,
//! differ from them only in the case of ASCII letters. The module's lists
//! are the ones the service held before it (its behaviour unchanged, the
//! amendment's *Effect*), and no service module or `cg-setup` module that
//! names the product's entries spells one of them itself.

use cg_agent::service::names::{
    self, product_name, product_name_str, same, same_str, ANCHOR, ATTEMPT, CERT, CONFIG,
    CONFIGURATION, CONSOLE_DATA_ROOT, DATA_ROOT, EXECUTABLE, IDENTITY, IDENTITY_DIRECTORY,
    IDENTITY_FILES, KEY, LEGACY, LOGS, LOGS_DIRECTORY, PRODUCT_ROOT, RECORD, RECORD_TEMPORARY,
    REQUIRED, REST, SERVICE_DIRECTORY, STAGED_ANCHOR, STAGED_CONFIG, STAGING, TEMPORARY,
};
use cg_agent::service::rotation::{archive_name, ACTIVE, ARCHIVES, ROTATING};

/// The lists as the service spelled them before the module
/// (`service/migration.rs`, `service/purge.rs` and `service/layout.rs`).
#[test]
fn install_ac_017_names_lists_are_the_services() {
    assert_eq!(PRODUCT_ROOT, "CyberGuard");
    assert_eq!(DATA_ROOT, "agent");
    assert_eq!(STAGING, "agent.migrating");
    assert_eq!(LEGACY, "agent.legacy");
    assert_eq!(IDENTITY_DIRECTORY, "identity");
    assert_eq!(LOGS_DIRECTORY, "logs");
    assert_eq!(SERVICE_DIRECTORY, "Agent");
    assert_eq!(EXECUTABLE, "cg-agent.exe");
    assert_eq!(TEMPORARY, "agent.toml.hygiene-tmp");
    assert_eq!(RECORD, "migration.json");
    assert_eq!(RECORD_TEMPORARY, "migration.json.rewrite-tmp");
    assert_eq!(STAGED_CONFIG, "agent.toml.setup");
    assert_eq!(STAGED_ANCHOR, "server-ca.pem.setup");
    assert_eq!(ATTEMPT, "setup-attempt.json");
    // migration.rs's REQUIRED and IDENTITY_FILES, layout.rs's CONSOLE_FILES.
    assert_eq!(
        REQUIRED,
        [
            "agent.toml",
            "server-ca.pem",
            "cert.pem",
            "key.dat",
            "identity.json"
        ]
    );
    assert_eq!(IDENTITY_FILES, ["cert.pem", "key.dat", "identity.json"]);
    assert_eq!(
        [CONFIG, ANCHOR, CERT, KEY, IDENTITY],
        REQUIRED,
        "the single names spell the list"
    );
    // migration.rs's known_name: REQUIRED, then the temporary; purge.rs's
    // LEGACY, `agent.toml` first.
    assert_eq!(
        CONSOLE_DATA_ROOT,
        [
            "agent.toml",
            "server-ca.pem",
            "cert.pem",
            "key.dat",
            "identity.json",
            "agent.toml.hygiene-tmp"
        ]
    );
    // purge.rs's CONFIGURATION, REST and LOGS, in the purge's order.
    assert_eq!(
        CONFIGURATION,
        ["agent.toml.setup", "agent.toml", "agent.toml.hygiene-tmp"]
    );
    assert_eq!(
        REST,
        [
            "server-ca.pem",
            "server-ca.pem.setup",
            "setup-attempt.json",
            "migration.json",
            "migration.json.rewrite-tmp"
        ]
    );
    assert_eq!(
        LOGS,
        [
            "agent.jsonl",
            "agent.1.jsonl",
            "agent.2.jsonl",
            "agent.3.jsonl",
            "agent.4.jsonl",
            "agent.5.jsonl",
            "agent.rotating.jsonl"
        ]
    );
    // The log's own names are the purge's.
    let mut rotation = vec![ACTIVE.to_string()];
    rotation.extend((1..=ARCHIVES).map(archive_name));
    rotation.push(ROTATING.to_string());
    assert_eq!(rotation, LOGS);
}

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// Point 4's comparison, on names another account could choose.
#[test]
fn install_ac_017_names_compare_utf16_units() {
    // (entry, product name, case-sensitive lookup, the product's)
    let cases: &[(&str, &str, bool, bool)] = &[
        ("agent.toml", CONFIG, false, true),
        ("agent.toml", CONFIG, true, true),
        ("AGENT.TOML", CONFIG, false, true),
        ("Agent.Toml", CONFIG, true, false),
        ("KEY.DAT", KEY, false, true),
        // A trailing dot or space is another name.
        ("agent.toml.", CONFIG, false, false),
        ("agent.toml ", CONFIG, false, false),
        // A soft hyphen, a right-to-left mark, a Cyrillic o.
        ("agent\u{AD}.toml", CONFIG, false, false),
        ("\u{200F}agent.toml", CONFIG, false, false),
        ("agent.t\u{43E}ml", CONFIG, false, false),
        // Unicode folds the Kelvin sign to k; ASCII case does not.
        ("\u{212A}ey.dat", KEY, false, false),
        // Characters a shell reads.
        ("agent.toml'", CONFIG, false, false),
        ("agent&.toml", CONFIG, false, false),
        ("%agent.toml%", CONFIG, false, false),
        ("agent", DATA_ROOT, false, true),
        ("agen", DATA_ROOT, false, false),
        ("agents", DATA_ROOT, false, false),
        ("", DATA_ROOT, false, false),
    ];
    for &(entry, name, case_sensitive, expected) in cases {
        assert_eq!(
            same(&units(entry), name, case_sensitive),
            expected,
            "{entry:?} against {name:?}, case-sensitive {case_sensitive}"
        );
        assert_eq!(
            same_str(entry, name, case_sensitive),
            expected,
            "{entry:?} against {name:?} as text"
        );
    }
    // An unpaired surrogate, which no `str` holds.
    let mut lone = units("agent.toml");
    lone.push(0xD800);
    assert!(!same(&lone, CONFIG, false));
    let mut lone = vec![0xDC00];
    lone.extend(units("agent.toml"));
    assert!(!same(&lone, CONFIG, false));
}

/// With case-sensitive lookup off, the comparison is the one the service
/// made before the module, `str::eq_ignore_ascii_case`, over every pair of
/// a corpus of names and product names.
#[test]
fn install_ac_017_names_keep_the_services_comparison() {
    let mut corpus: Vec<String> = Vec::new();
    for name in names::ALL {
        corpus.push(name.to_string());
        corpus.push(name.to_ascii_uppercase());
        corpus.push(format!("{name}."));
        corpus.push(format!("{name}\u{AD}"));
        corpus.push(name.replacen('a', "\u{430}", 1));
        corpus.push(name.replacen('k', "\u{212A}", 1));
    }
    for entry in &corpus {
        for name in names::ALL {
            assert_eq!(
                same_str(entry, name, false),
                entry.eq_ignore_ascii_case(name),
                "{entry:?} against {name:?}"
            );
        }
    }
}

/// `product_name` gives the product's spelling of the name an entry is.
#[test]
fn install_ac_017_names_product_name() {
    assert_eq!(
        product_name(&CONSOLE_DATA_ROOT, &units("Key.Dat"), false),
        Some(KEY)
    );
    assert_eq!(
        product_name(&CONSOLE_DATA_ROOT, &units("Key.Dat"), true),
        None
    );
    assert_eq!(
        product_name_str(&CONSOLE_DATA_ROOT, "AGENT.TOML.HYGIENE-TMP", false),
        Some(TEMPORARY)
    );
    assert_eq!(
        product_name_str(&CONSOLE_DATA_ROOT, "migration.json", false),
        None
    );
    assert_eq!(
        product_name_str(&REST, "Migration.Json", false),
        Some(RECORD)
    );
}

/// Every name of the module is in `ALL`, once.
#[test]
fn install_ac_017_names_all_is_complete() {
    let mut listed: Vec<&str> = names::ALL.to_vec();
    for list in [
        &REQUIRED[..],
        &IDENTITY_FILES[..],
        &CONSOLE_DATA_ROOT[..],
        &CONFIGURATION[..],
        &REST[..],
        &LOGS[..],
    ] {
        for name in list {
            assert!(listed.contains(name), "{name} is not in ALL");
        }
    }
    let count = listed.len();
    listed.sort_unstable();
    listed.dedup();
    assert_eq!(listed.len(), count, "ALL holds a name twice");
}

/// The modules that name the product's entries: the service's and the
/// setup's, read as source.
const SOURCES: &[(&str, &str)] = &[
    (
        "service/layout.rs",
        include_str!("../src/service/layout.rs"),
    ),
    (
        "service/migration.rs",
        include_str!("../src/service/migration.rs"),
    ),
    ("service/purge.rs", include_str!("../src/service/purge.rs")),
    (
        "service/adoption.rs",
        include_str!("../src/service/adoption.rs"),
    ),
    (
        "service/log_file.rs",
        include_str!("../src/service/log_file.rs"),
    ),
    (
        "service/rotation.rs",
        include_str!("../src/service/rotation.rs"),
    ),
    ("service/mod.rs", include_str!("../src/service/mod.rs")),
    (
        "cg-setup/src/verbs.rs",
        include_str!("../../cg-setup/src/verbs.rs"),
    ),
    (
        "cg-setup/src/record.rs",
        include_str!("../../cg-setup/src/record.rs"),
    ),
];

/// The code of a source, its comment lines left out.
fn code_lines(source: &str) -> impl Iterator<Item = (usize, &str)> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .map(|(index, line)| (index + 1, line))
}

/// A string equal to a product name that is no name of an entry: the
/// configuration's `[agent]` table, which the migration's generated
/// configuration writes (the Amendment 2026-11-05, item 2.5).
const NOT_AN_ENTRY: &[&str] = &["table_of(&mut document, \"agent\")"];

/// No module spells a product name as a string of its own: each takes it
/// from the names module.
#[test]
fn install_ac_017_names_one_module() {
    let mut spelled = Vec::new();
    for (file, source) in SOURCES {
        for (line, code) in code_lines(source) {
            if NOT_AN_ENTRY.iter().any(|except| code.contains(except)) {
                continue;
            }
            for name in names::ALL {
                if code.contains(&format!("\"{name}\"")) {
                    spelled.push(format!("{file}:{line} spells {name:?}"));
                }
            }
        }
    }
    assert!(spelled.is_empty(), "{spelled:#?}");
}

/// The service's comparisons of an entry with a product name go through the
/// module: `eq_ignore_ascii_case` remains only where a configured path is
/// compared with a fixed one (the Amendment 2026-11-04, item 2), which is a
/// path, not an entry.
#[test]
fn install_ac_017_names_compared_in_one_place() {
    let mut compared = Vec::new();
    for (file, source) in SOURCES {
        for (line, code) in code_lines(source) {
            let path_comparison = code.contains("fixed.eq_ignore_ascii_case");
            if code.contains("eq_ignore_ascii_case") && !path_comparison {
                compared.push(format!("{file}:{line}: {}", code.trim()));
            }
        }
    }
    assert!(compared.is_empty(), "{compared:#?}");
}
