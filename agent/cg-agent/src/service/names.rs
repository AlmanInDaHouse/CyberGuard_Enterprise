//! The product's names (SPEC-024 Amendment 2026-11-19, point 4): the entries
//! of `<P>`, the data root, the staging and the legacy directory, and of the
//! service's directory under `%ProgramFiles%\CyberGuard`, for each layout, held
//! here for the migration, the purge, the reading of the host's layout, the
//! setup and its custody verb.
//!
//! A name is the product's when its UTF-16 units equal those of a product
//! name or, in a directory whose case-sensitive lookup is off, differ from
//! them only in the case of ASCII letters: no Unicode folding, no
//! normalisation, no trimming of a trailing dot or space. The service's
//! callers pass `case_sensitive` off, as they compared before this module;
//! the migration refuses a data root whose lookup is on (the Amendment
//! 2026-11-05, item 2.2).

/// `%ProgramData%\CyberGuard`, `<P>`, and `%ProgramFiles%\CyberGuard`.
pub const PRODUCT_ROOT: &str = "CyberGuard";
/// The data root in `<P>`.
pub const DATA_ROOT: &str = "agent";
/// The migration's staging directory beside the data root (the Amendment
/// 2026-11-05).
pub const STAGING: &str = "agent.migrating";
/// The console installation's directory after the migration's commit.
pub const LEGACY: &str = "agent.legacy";
/// The service's identity directory in the data root.
pub const IDENTITY_DIRECTORY: &str = "identity";
/// The service's log directory in the data root.
pub const LOGS_DIRECTORY: &str = "logs";
/// The service's directory under `%ProgramFiles%\CyberGuard`.
pub const SERVICE_DIRECTORY: &str = "Agent";
/// The agent's executable, the service's in `SERVICE_DIRECTORY` and the
/// console's in `%ProgramFiles%\CyberGuard`.
pub const EXECUTABLE: &str = "cg-agent.exe";

pub const CONFIG: &str = "agent.toml";
pub const ANCHOR: &str = "server-ca.pem";
pub const CERT: &str = "cert.pem";
pub const KEY: &str = "key.dat";
pub const IDENTITY: &str = "identity.json";
/// FR-014's leftover: never read or adopted, restricted as a copy.
pub const TEMPORARY: &str = "agent.toml.hygiene-tmp";
/// The migration's record, in the staging directory and, after the commit,
/// the data root.
pub const RECORD: &str = "migration.json";
/// `migration.json`'s rewrite goes through this name beside it.
pub const RECORD_TEMPORARY: &str = "migration.json.rewrite-tmp";
/// The configuration the setup staged (the Amendment 2026-11-09, item 3).
pub const STAGED_CONFIG: &str = "agent.toml.setup";
/// The trust anchor the setup staged.
pub const STAGED_ANCHOR: &str = "server-ca.pem.setup";
/// The setup's record of one preflight (the Amendment 2026-11-10, item 5).
pub const ATTEMPT: &str = "setup-attempt.json";

/// A migration in progress as a refusal names its layout (SPEC-024 Amendment
/// 2026-11-20, point 3): the staging directory, or the legacy directory with
/// no data root.
pub fn migration_layout() -> String {
    format!("{STAGING}, or {LEGACY} without {DATA_ROOT}")
}

/// The identity files: in a console installation's data root, which they
/// make one, and in the service's identity directory.
pub const IDENTITY_FILES: [&str; 3] = [CERT, KEY, IDENTITY];
/// What a console installation's data root must hold.
pub const REQUIRED: [&str; 5] = [CONFIG, ANCHOR, CERT, KEY, IDENTITY];
/// What a console installation's data root may hold, and so the legacy
/// directory after the commit, `agent.toml` first (the Amendment 2026-11-05,
/// item 2.2).
pub const CONSOLE_DATA_ROOT: [&str; 6] = [CONFIG, ANCHOR, CERT, KEY, IDENTITY, TEMPORARY];
/// The purge's first step, the staged configuration before `agent.toml`
/// (the Amendment 2026-11-15, point 6).
pub const CONFIGURATION: [&str; 3] = [STAGED_CONFIG, CONFIG, TEMPORARY];
/// The service's data root's other files.
pub const REST: [&str; 5] = [ANCHOR, STAGED_ANCHOR, ATTEMPT, RECORD, RECORD_TEMPORARY];
/// The log directory's files: the active file, its five archives and the
/// active file while it rotates (`rotation`).
pub const LOGS: [&str; 7] = [
    "agent.jsonl",
    "agent.1.jsonl",
    "agent.2.jsonl",
    "agent.3.jsonl",
    "agent.4.jsonl",
    "agent.5.jsonl",
    "agent.rotating.jsonl",
];

/// Every name above, once.
pub const ALL: [&str; 26] = [
    PRODUCT_ROOT,
    DATA_ROOT,
    STAGING,
    LEGACY,
    IDENTITY_DIRECTORY,
    LOGS_DIRECTORY,
    SERVICE_DIRECTORY,
    EXECUTABLE,
    CONFIG,
    ANCHOR,
    CERT,
    KEY,
    IDENTITY,
    TEMPORARY,
    RECORD,
    RECORD_TEMPORARY,
    STAGED_CONFIG,
    STAGED_ANCHOR,
    ATTEMPT,
    LOGS[0],
    LOGS[1],
    LOGS[2],
    LOGS[3],
    LOGS[4],
    LOGS[5],
    LOGS[6],
];

/// An ASCII capital letter as its small letter; any other unit as it is.
fn fold(unit: u16) -> u16 {
    if (u16::from(b'A')..=u16::from(b'Z')).contains(&unit) {
        unit + u16::from(b'a' - b'A')
    } else {
        unit
    }
}

fn same_units(mut entry: impl Iterator<Item = u16>, name: &str, case_sensitive: bool) -> bool {
    let mut wanted = name.encode_utf16();
    loop {
        match (entry.next(), wanted.next()) {
            (None, None) => return true,
            (Some(unit), Some(want))
                if unit == want || (!case_sensitive && fold(unit) == fold(want)) => {}
            _ => return false,
        }
    }
}

/// Whether `entry`, a name as its directory stores it, is `name` (point 4).
pub fn same(entry: &[u16], name: &str, case_sensitive: bool) -> bool {
    same_units(entry.iter().copied(), name, case_sensitive)
}

/// `same` of a name read as text.
pub fn same_str(entry: &str, name: &str, case_sensitive: bool) -> bool {
    same_units(entry.encode_utf16(), name, case_sensitive)
}

/// The name of `list` that `entry` is, in the product's spelling.
pub fn product_name(
    list: &[&'static str],
    entry: &[u16],
    case_sensitive: bool,
) -> Option<&'static str> {
    list.iter()
        .copied()
        .find(|name| same(entry, name, case_sensitive))
}

/// `product_name` of a name read as text.
pub fn product_name_str(
    list: &[&'static str],
    entry: &str,
    case_sensitive: bool,
) -> Option<&'static str> {
    list.iter()
        .copied()
        .find(|name| same_str(entry, name, case_sensitive))
}
