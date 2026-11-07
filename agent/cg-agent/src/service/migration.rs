//! The legacy migration (SPEC-024 Amendment 2026-11-05, install_ac_013): the
//! service, as SYSTEM, moves a host the console runbook set up into SYSTEM
//! custody during `START_PENDING`, asked by its SCM start arguments.
//!
//! `prepare` is what a start does before it reads `agent.toml`: it reads
//! which of the data root, the staging and the legacy directory exist, runs
//! the route of item 2 or the completion of item 4, and checks the data
//! directories. `copies` is item 5, after `RUNNING`. Everything takes an
//! `Environment`, so that the tests point it at a temporary root with the
//! current user in SYSTEM's place and inject a failure at each `Point`.
//!
//! The route closes `<P>` and the data root first, so that from then on no
//! ordinary user's handle can create an entry in them; it reads each file once,
//! through a handle that excludes writers, and works from those bytes. It
//! restricts the console installation in place, so that a refusal leaves one
//! that an elevated console agent still runs, and commits by two renames.
//! Which step an interrupted start reached is read from which directories
//! exist; `migration.json` holds the operator's request and what no
//! directory can say.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use rustls::pki_types::pem::PemObject as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use windows_sys::Win32::Foundation::GENERIC_ALL;
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL, WRITE_DAC, WRITE_OWNER,
};
use zeroize::Zeroizing;

use super::{
    check_installed_config, config_error_line, log_file, stderr_service_log, InstalledPaths,
    EXIT_SERVICE_LOG,
};
use crate::config::AgentConfig;
use crate::crypto::{pubkey_fingerprint, AgentKeypair, KEY_LEN};
use crate::custody::{self, Descriptor, Kind, Policy, Principal, Unread};
use crate::errors::{EnrollmentError, TlsError};
use crate::identity::{check_consistency, installed, PersistedIdentity};
use crate::secure_storage::default_store;

/// The reason a start without migration arguments adds on a console layout
/// (item 6).
pub const DEFAULT_REFUSAL: &str = "the data directory holds a console installation's identity files: migrate them before removing anything";

/// How long an operation may wait on another process.
pub const OPERATION_WAIT: Duration = custody::OPEN_WAIT;

const CONFIG: &str = "agent.toml";
const ANCHOR: &str = "server-ca.pem";
const CERT: &str = "cert.pem";
const KEY: &str = "key.dat";
const IDENTITY: &str = "identity.json";
const IDENTITY_DIRECTORY: &str = "identity";
/// FR-014's leftover: never read or adopted, restricted as a copy.
const TEMPORARY: &str = "agent.toml.hygiene-tmp";
const RECORD: &str = "migration.json";
/// `migration.json`'s rewrite goes through this name beside it.
const RECORD_TEMPORARY: &str = "migration.json.rewrite-tmp";
/// What a console layout's data root must hold.
const REQUIRED: [&str; 5] = [CONFIG, ANCHOR, CERT, KEY, IDENTITY];
/// The most item 2.3 reads of a file: far above anything the agent writes,
/// and a bound on what the enrolling account's replaced file can cost.
const READ_LIMIT: u64 = 16 * 1024 * 1024;
const IDENTITY_FILES: [&str; 3] = [CERT, KEY, IDENTITY];

/// Any access an unrelated handle may hold while the service opens an object
/// for its attributes or its security.
const SHARE_ALL: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
/// What changing an object's security through a handle needs.
const SECURITY_ACCESS: u32 = READ_CONTROL | WRITE_DAC | WRITE_OWNER | FILE_READ_ATTRIBUTES;
/// What reading an object's attributes and security needs.
const INSPECT_ACCESS: u32 = READ_CONTROL | FILE_READ_ATTRIBUTES;

/// Item 2.2's repairs. The lines name the repair, never a SID.
const REPAIR_ENTRY: &str =
    "repair: an administrator who has checked that it is not the agent's removes it";
const REPAIR_LINK: &str = "repair: an administrator gives it owner SYSTEM and a protected DACL for SYSTEM and Administrators, so that no one else can link it again, then removes its other names";
const REPAIR_OWNER: &str = "repair: after a restart, before that account signs in again, an elevated administrator of another account who has checked that the owner is the account that set up or enrolled the agent makes Administrators its owner, and gives a directory SYSTEM and Administrators alone, then starts the migration again; an owner that cannot be so confirmed means enrolling anew";
const REPAIR_TEMPORARY: &str =
    "repair: an administrator deletes it, since it holds at most a consumed token";
/// Item 6's one route for what item 2 has no repair for.
const REPAIR_ANEW: &str = "repair: an administrator keeps the files aside, removes the CyberGuard directory and installs anew with a new token, the old agent removed on the server";

/// The operator's choice for the old copies (item 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Choice {
    Remove,
    Keep,
}

/// What the start arguments ask for (item 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub choice: Choice,
    pub fingerprints: BTreeSet<String>,
}

fn invalid_arguments(reason: &str) -> String {
    format!("cg-agent: invalid migration arguments: {reason}")
}

/// `sha256:` and 64 lowercase hexadecimal digits (§Data contracts §3).
fn is_fingerprint(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digits| {
        digits.len() == 64
            && digits
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// The SCM start arguments after the service's name (item 1): `None` when
/// there are none; the error is code 2's whole line, which never repeats a
/// value.
pub fn parse_start_arguments(arguments: &[String]) -> Result<Option<Request>, String> {
    if arguments.is_empty() {
        return Ok(None);
    }
    let mut choice = None;
    let mut fingerprints = BTreeSet::new();
    for argument in arguments {
        if let Some(value) = argument.strip_prefix("--migrate-legacy=") {
            let parsed = match value {
                "remove" => Choice::Remove,
                "keep" => Choice::Keep,
                _ => return Err(invalid_arguments("--migrate-legacy is not remove or keep")),
            };
            if choice.replace(parsed).is_some() {
                return Err(invalid_arguments(
                    "--migrate-legacy is given more than once",
                ));
            }
        } else if let Some(value) = argument.strip_prefix("--trust-anchor-sha256=") {
            if !is_fingerprint(value) {
                return Err(invalid_arguments(
                    "--trust-anchor-sha256 is not sha256: and 64 lowercase hexadecimal digits",
                ));
            }
            fingerprints.insert(value.to_string());
        } else {
            return Err(invalid_arguments(
                "a start argument is neither --migrate-legacy nor --trust-anchor-sha256",
            ));
        }
    }
    let Some(choice) = choice else {
        return Err(invalid_arguments(
            "--trust-anchor-sha256 is given without --migrate-legacy",
        ));
    };
    if fingerprints.is_empty() {
        return Err(invalid_arguments(
            "--migrate-legacy is given without --trust-anchor-sha256",
        ));
    }
    Ok(Some(Request {
        choice,
        fingerprints,
    }))
}

/// `migration.json` (item 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub choice: Choice,
    pub fingerprints: Vec<String>,
    pub agent_id: String,
    pub key_fingerprint: String,
    /// The enrolling account's SID as text; `None` when it is unknown.
    pub enrolling_account: Option<String>,
    /// Each checked file's name and its file ID.
    pub files: BTreeMap<String, String>,
    /// A reversal of a failed second rename is under way (item 2.6).
    pub reversal_pending: bool,
    /// The copies item 5 has not reached.
    pub copies_not_reached: Vec<String>,
}

impl Record {
    fn asks_for(&self, request: &Request) -> bool {
        self.choice == request.choice
            && self.fingerprints.iter().collect::<BTreeSet<_>>()
                == request.fingerprints.iter().collect::<BTreeSet<_>>()
    }
}

/// Who and what the migration checks and writes. The tests point it at the
/// current user where the service's names SYSTEM.
#[derive(Debug, Clone)]
pub struct MigrationCustody {
    /// SYSTEM: an identity file's DACL must allow it (item 2.2).
    pub service: Principal,
    /// Who may own what items 2.1 and 2.2 check.
    pub owners: Vec<Principal>,
    /// Who else may change a directory item 2.1 closes, or take its
    /// ownership.
    pub trusted: Vec<Principal>,
    /// `agent.toml`'s policy (Amendment 2026-11-04, item 3): also the
    /// staging and legacy directories' and `migration.json`'s.
    pub configuration: Policy,
    /// `identity\` and its files.
    pub identity: Policy,
    /// Item 2.1's directories and the staging directory.
    pub closed_directory_sddl: String,
    /// Item 2.4's files, and `agent.toml`, `server-ca.pem` and
    /// `migration.json` when staged.
    pub shared_file_sddl: String,
    /// `identity\` and the legacy directory.
    pub identity_directory_sddl: String,
    /// The identity's files, staged or old.
    pub identity_file_sddl: String,
}

impl MigrationCustody {
    /// The service's.
    pub fn system() -> Self {
        let both = vec![Principal::System, Principal::Administrators];
        MigrationCustody {
            service: Principal::System,
            owners: both.clone(),
            trusted: both,
            configuration: Policy::configuration(),
            identity: Policy::system_only(),
            closed_directory_sddl: log_file::PRIVATE_DIRECTORY.to_string(),
            shared_file_sddl: "O:SYD:P(A;;FA;;;SY)(A;;FA;;;BA)".to_string(),
            identity_directory_sddl: installed::DIRECTORY_SDDL.to_string(),
            identity_file_sddl: installed::FILE_SDDL.to_string(),
        }
    }
}

/// Where a start stands, for `Watch`: each point comes immediately before
/// the operation it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Point {
    /// Item 2.1: `<P>`, then the data root.
    CloseProductRoot,
    CloseDataRoot,
    /// Item 4: deleting a staging directory without a record.
    DeleteInterruptedStaging,
    /// Item 2.2: inspecting the file of that name.
    Check(&'static str),
    /// Item 2.3: reading the file of that name.
    Read(&'static str),
    /// Item 2.4: restricting the file of that name, and reading its link
    /// count again once it is restricted.
    Restrict(&'static str),
    Restricted(&'static str),
    /// Item 2.5.
    CreateStaging,
    WriteRecord,
    /// Writing the staged object of that name: `agent.toml`,
    /// `server-ca.pem`, `identity`, `cert.pem`, `key.dat`, `identity.json`.
    Stage(&'static str),
    CheckStaged,
    /// Item 2.6: the first rename and the listing just before it.
    FirstRename,
    SecondRename,
    RecordReversal,
    RenameBack,
    ClearReversal,
    /// Item 4, completing the commit of an interrupted migration.
    CompleteCommit,
    /// Item 5: the legacy directory, its object of that name, the removal
    /// of the emptied directory and the record's rewrite.
    RestrictLegacyDirectory,
    Copy(&'static str),
    RemoveLegacyDirectory,
    RewriteRecord,
}

/// What an injected point does: `Failure` makes the operation fail with
/// that reason, as if Windows had refused it; `Crash` ends the start there,
/// with nothing after it run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Injected {
    Failure(String),
    Crash,
}

impl From<String> for Injected {
    fn from(reason: String) -> Self {
        Injected::Failure(reason)
    }
}

/// Told of each point. The service advances its checkpoint; the tests
/// inject failures, with no Cargo feature.
pub trait Watch: Sync {
    fn at(&self, point: Point) -> Result<(), Injected>;
}

/// What the start is told by the migration.
pub struct Environment<'a> {
    pub paths: InstalledPaths,
    pub custody: MigrationCustody,
    /// `identity.json`'s `expires_at` must be after this (item 2.3).
    pub now: DateTime<Utc>,
    /// How long an operation may wait on another process (30 seconds).
    pub wait: Duration,
    pub watch: &'a (dyn Watch + 'a),
}

impl<'a> Environment<'a> {
    /// The service's, at the fixed paths.
    pub fn service(paths: InstalledPaths, watch: &'a (dyn Watch + 'a)) -> Self {
        Environment {
            paths,
            custody: MigrationCustody::system(),
            now: Utc::now(),
            wait: OPERATION_WAIT,
            watch,
        }
    }

    fn staged_record(&self) -> PathBuf {
        self.paths.staging().join(RECORD)
    }
}

/// Why a start ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// The code and the cause's whole line.
    Refused { code: u8, line: String },
    /// An injected crash: nothing after that point ran.
    Crashed,
}

/// How the start goes on after `prepare`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prepared {
    /// No migration and no console layout: the Amendment 2026-11-04's
    /// start.
    Plain,
    /// A console layout with no migration in progress and no arguments:
    /// its checks stand, each refusal's reason adding `DEFAULT_REFUSAL`
    /// (item 6). The data directories' check passed.
    ConsoleLayout,
    /// A migration committed, on this start or before: the start runs and,
    /// after `RUNNING`, `copies` continues (item 5).
    Committed,
}

fn identity_line(reason: &str) -> String {
    EnrollmentError::Persistence(reason.to_string()).stderr_line()
}

fn anchor_line(reason: &str) -> String {
    TlsError::ClientConfig(format!("migration: {reason}")).stderr_line()
}

fn refused(code: u8, line: String) -> Stop {
    Stop::Refused { code, line }
}

/// How long the line of a refusal waits to read an object's security.
const REFUSAL_READ_WAIT: Duration = Duration::from_secs(1);

/// What a refusal before the commit adds: the console installation is in
/// place, and what items 2.1 and 2.4 restricted of it, on this start or an
/// earlier one, read from the objects themselves, so that it holds whatever
/// the refused start did.
fn with_state(environment: &Environment<'_>, line: String) -> String {
    let paths = &environment.paths;
    let policy = &environment.custody.configuration;
    let wait = environment.wait.min(REFUSAL_READ_WAIT);
    // Through a reparse point at either directory these would be other
    // objects.
    let root_plain = custody::is_plain_directory(&paths.product_root, wait);
    let plain = root_plain && custody::is_plain_directory(&paths.data_root, wait);
    let directories = [(&paths.product_root, true), (&paths.data_root, root_plain)]
        .into_iter()
        .filter(|(path, plain)| *plain && custody::has_security(path, policy, wait))
        .map(|(path, _)| path.display().to_string());
    let files = REQUIRED
        .iter()
        .chain(std::iter::once(&TEMPORARY))
        .filter(|name| plain && custody::has_security(&paths.data_root.join(name), policy, wait))
        .map(|name| name.to_string());
    let restricted: Vec<String> = directories.chain(files).collect();
    if restricted.is_empty() {
        format!("{line}; the console installation is in place, none of it restricted")
    } else {
        format!(
            "{line}; the console installation is in place, restricted: {}",
            restricted.join(", ")
        )
    }
}

fn in_place(environment: &Environment<'_>, code: u8, line: String) -> Stop {
    refused(code, with_state(environment, line))
}

/// A failed step's stop: `render` makes the cause's line of its reason.
fn cut_in_place(
    environment: &Environment<'_>,
    cut: Injected,
    code: u8,
    render: impl FnOnce(&str) -> String,
) -> Stop {
    match cut {
        Injected::Crash => Stop::Crashed,
        Injected::Failure(reason) => in_place(environment, code, render(&reason)),
    }
}

/// A step's failure names its object, whatever the operation's own reason
/// says.
fn naming(path: &Path, cut: Injected) -> Injected {
    match cut {
        Injected::Failure(reason) if !reason.contains(&path.display().to_string()) => {
            Injected::Failure(format!("{}: {reason}", path.display()))
        }
        other => other,
    }
}

/// A name is there, whatever it is; nothing reparse is followed for the
/// last component. A name that cannot be inspected counts as there, so that
/// it is checked rather than taken as absent.
fn is_there(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(_) => true,
        Err(e) => e.kind() != std::io::ErrorKind::NotFound,
    }
}

/// Item 4: a staging or legacy directory counts only as a directory, not a
/// reparse point, under `agent.toml`'s policy: a name an ordinary user
/// created cannot pass, and changes nothing.
fn counts(environment: &Environment<'_>, path: &Path) -> bool {
    matches!(
        custody::check_if_exists_within(
            path,
            Kind::Directory,
            &environment.custody.configuration,
            environment.wait
        ),
        Ok(true)
    )
}

/// The record of a staging directory that counts: what passes
/// `agent.toml`'s policy and parses, or nothing, which a torn file is. A
/// record that cannot be read now is neither, and the error is why: the
/// start stops, so that it is not taken for one that is not there and its
/// directory deleted.
fn staged_record(environment: &Environment<'_>) -> Result<Option<Record>, String> {
    match custody::read_checked_within(
        &environment.staged_record(),
        &environment.custody.configuration,
        environment.wait,
    ) {
        Ok(Some(bytes)) => Ok(serde_json::from_slice(&bytes).ok()),
        Ok(None) | Err(Unread::Refused(_)) => Ok(None),
        Err(Unread::Unreadable(reason)) => Err(format!(
            "the migration's record cannot be read: {reason}; nothing was changed"
        )),
    }
}

/// The data root's record: after the commit it passes `agent.toml`'s policy
/// and parses, or the start refuses it like a configuration file (item 3).
fn data_root_record(environment: &Environment<'_>) -> Result<Option<Record>, Stop> {
    let path = environment.paths.record();
    let invalid = |reason: String| {
        refused(
            2,
            format!("cg-agent: invalid config {}: {reason}", path.display()),
        )
    };
    let bytes =
        custody::read_checked_within(&path, &environment.custody.configuration, environment.wait)
            .map_err(|unread| match unread {
            Unread::Unreadable(reason) | Unread::Refused(reason) => invalid(reason),
        })?;
    match bytes {
        None => Ok(None),
        Some(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| invalid("the record does not parse".to_string())),
    }
}

const NOT_THE_RECORDED: &str = "they are not the ones the migration in progress recorded";

fn check_recorded(request: Option<&Request>, record: &Record) -> Result<(), Stop> {
    match request {
        Some(request) if !record.asks_for(request) => {
            Err(refused(2, invalid_arguments(NOT_THE_RECORDED)))
        }
        _ => Ok(()),
    }
}

/// The Amendment 2026-11-04, item 3's check of `<P>` and the data root:
/// code 13. On a console layout its reason adds item 6's.
fn check_data_directories(environment: &Environment<'_>, console: bool) -> Result<(), Stop> {
    let policy = Policy {
        owners: environment.custody.owners.clone(),
        allowed: environment.custody.configuration.allowed.clone(),
        protected: false,
    };
    for directory in [
        &environment.paths.product_root,
        &environment.paths.data_root,
    ] {
        let checked =
            custody::check_if_exists_within(directory, Kind::Directory, &policy, environment.wait);
        if let Err(reason) = checked {
            let reason = if console {
                format!("{reason}; {DEFAULT_REFUSAL}")
            } else {
                reason
            };
            return Err(refused(EXIT_SERVICE_LOG, stderr_service_log(&reason)));
        }
    }
    Ok(())
}

fn other_combination(environment: &Environment<'_>) -> Stop {
    refused(
        5,
        identity_line(&format!(
            "{}, {} and {} are in no state a migration leaves; nothing was changed",
            environment.paths.data_root.display(),
            environment.paths.staging().display(),
            environment.paths.legacy().display()
        )),
    )
}

/// What a start does before it reads the configuration (Amendment
/// 2026-11-05, items 1 to 4 and 6): after the checks of `--config`, read
/// which directories exist, run the route or the completion a request or
/// an interrupted migration calls for, and check the data directories
/// (Amendment 2026-11-04, item 3). `lock` takes the instance lock; it is
/// called before any change, and at most once.
pub fn prepare(
    environment: &Environment<'_>,
    request: Option<&Request>,
    lock: &mut dyn FnMut() -> Result<(), Stop>,
) -> Result<Prepared, Stop> {
    let paths = &environment.paths;
    let data_root = is_there(&paths.data_root);
    let console = data_root
        && IDENTITY_FILES
            .iter()
            .any(|name| is_there(&paths.data_root.join(name)));
    let staging = counts(environment, &paths.staging());
    let legacy = counts(environment, &paths.legacy());
    match (data_root, staging, legacy) {
        (false, true, true) => between_the_renames(environment, request, lock),
        (true, true, false) if console => beside_a_console_layout(environment, request, lock),
        (true, false, true) if !console => committed(environment, request),
        (_, false, false) if console => match request {
            Some(request) => route(environment, request, Attempt::Fresh, lock),
            None => {
                check_data_directories(environment, true)?;
                Ok(Prepared::ConsoleLayout)
            }
        },
        (_, false, false) => plain(environment, request),
        _ => Err(other_combination(environment)),
    }
}

/// A data root alone, or none of the three, without a console layout.
fn plain(environment: &Environment<'_>, request: Option<&Request>) -> Result<Prepared, Stop> {
    if request.is_some() {
        return Err(refused(
            2,
            invalid_arguments("this host is neither a console layout nor a migration in progress"),
        ));
    }
    check_data_directories(environment, false)?;
    data_root_record(environment)?;
    Ok(Prepared::Plain)
}

/// A data root that is not a console layout, beside a legacy directory.
fn committed(environment: &Environment<'_>, request: Option<&Request>) -> Result<Prepared, Stop> {
    check_data_directories(environment, false)?;
    let Some(record) = data_root_record(environment)? else {
        return Err(other_combination(environment));
    };
    check_recorded(request, &record)?;
    Ok(Prepared::Committed)
}

/// A staging directory beside a console-layout data root: an interruption
/// before the first rename or after a reversal, or, without a record, the
/// service's own creation interrupted.
fn beside_a_console_layout(
    environment: &Environment<'_>,
    request: Option<&Request>,
    lock: &mut dyn FnMut() -> Result<(), Stop>,
) -> Result<Prepared, Stop> {
    let staging = environment.paths.staging();
    let record = staged_record(environment).map_err(|reason| {
        in_place(
            environment,
            5,
            identity_line(&format!(
                "{reason}; if it stays so, an administrator who has checked it removes the staging directory and starts the migration again"
            )),
        )
    })?;
    match (record, request) {
        (Some(record), Some(request)) if !record.asks_for(request) => {
            Err(in_place(environment, 2, invalid_arguments(NOT_THE_RECORDED)))
        }
        (Some(record), Some(request)) => {
            route(environment, request, Attempt::Continued(record), lock)
        }
        (None, Some(request)) => route(environment, request, Attempt::Interrupted, lock),
        (Some(_), None) => Err(in_place(
            environment,
            5,
            identity_line(&format!(
                "a migration was interrupted, its staging directory at {}: start the service again with the arguments it recorded",
                staging.display()
            )),
        )),
        (None, None) => Err(in_place(
            environment,
            5,
            identity_line(&format!(
                "the service's creation of {} was interrupted: start the migration again",
                staging.display()
            )),
        )),
    }
}

/// No data root, a staging and a legacy directory: a crash between the
/// renames, or a reversal to finish.
fn between_the_renames(
    environment: &Environment<'_>,
    request: Option<&Request>,
    lock: &mut dyn FnMut() -> Result<(), Stop>,
) -> Result<Prepared, Stop> {
    let record =
        staged_record(environment).map_err(|reason| both_directories(environment, &reason))?;
    let Some(record) = record else {
        return Err(both_directories(environment, "its record cannot be read"));
    };
    check_recorded(request, &record)?;
    lock()?;
    if record.reversal_pending {
        return Err(reverse(environment, record, "a reversal was interrupted"));
    }
    let completed = (|| -> Result<(), Injected> {
        check_staged(environment, &record)?;
        environment.watch.at(Point::CompleteCommit)?;
        custody::rename(
            &environment.paths.staging(),
            &environment.paths.data_root,
            environment.wait,
        )?;
        Ok(())
    })();
    match completed {
        Ok(()) => {
            check_data_directories(environment, false)?;
            Ok(Prepared::Committed)
        }
        Err(Injected::Crash) => Err(Stop::Crashed),
        Err(Injected::Failure(reason)) => Err(both_directories(environment, &reason)),
    }
}

/// Code 5 for an interrupted commit that cannot go on, naming both
/// directories.
fn both_directories(environment: &Environment<'_>, reason: &str) -> Stop {
    refused(
        5,
        identity_line(&format!(
            "a migration stopped between its renames, at {} and {}: {reason}",
            environment.paths.staging().display(),
            environment.paths.legacy().display()
        )),
    )
}

/// Rename the legacy directory back and clear the reversal's mark (items
/// 2.6 and 4). The mark is already in the record.
fn reverse(environment: &Environment<'_>, mut record: Record, cause: &str) -> Stop {
    let paths = &environment.paths;
    let back = (|| -> Result<(), Injected> {
        environment.watch.at(Point::RenameBack)?;
        custody::rename(&paths.legacy(), &paths.data_root, environment.wait)?;
        Ok(())
    })();
    match back {
        Ok(()) => {}
        Err(Injected::Crash) => return Stop::Crashed,
        Err(Injected::Failure(reason)) => {
            return refused(
                5,
                identity_line(&format!(
                    "{cause}, and renaming {} back failed ({reason}): the data root is at that name, and the next start, with or without arguments, puts it back",
                    paths.legacy().display()
                )),
            )
        }
    }
    record.reversal_pending = false;
    let cleared = (|| -> Result<(), Injected> {
        environment.watch.at(Point::ClearReversal)?;
        rewrite(environment, &environment.staged_record(), &record)?;
        Ok(())
    })();
    if cleared == Err(Injected::Crash) {
        return Stop::Crashed;
    }
    // A mark left behind beside a console layout changes nothing: the
    // continued migration clears it (item 2.5).
    in_place(
        environment,
        5,
        identity_line(&format!("{cause}; the commit was reversed")),
    )
}

/// Rewrite a record as the Amendment 2026-11-04, item 6, rewrites
/// `agent.toml`, keeping its owner and DACL.
fn rewrite(environment: &Environment<'_>, path: &Path, record: &Record) -> Result<(), String> {
    let bytes =
        serde_json::to_vec_pretty(record).map_err(|e| format!("write {}: {e}", path.display()))?;
    let temporary = path.with_file_name(RECORD_TEMPORARY);
    let policy = environment.custody.configuration.clone();
    let owned = path.to_path_buf();
    custody::bounded(environment.wait, path, move || {
        custody::replace_keeping_security(&owned, &temporary, &bytes, &policy)
    })
}

/// Which start of a migration this is.
enum Attempt {
    Fresh,
    /// A staging directory without a record: deleted first.
    Interrupted,
    /// A staging directory and its record.
    Continued(Record),
}

impl Attempt {
    fn recorded(&self) -> Option<&Record> {
        match self {
            Attempt::Continued(record) => Some(record),
            _ => None,
        }
    }
}

/// Item 2.
fn route(
    environment: &Environment<'_>,
    request: &Request,
    attempt: Attempt,
    lock: &mut dyn FnMut() -> Result<(), Stop>,
) -> Result<Prepared, Stop> {
    lock().map_err(|stop| match stop {
        Stop::Refused { code, line } => in_place(environment, code, line),
        crashed => crashed,
    })?;
    close_directories(environment)?;
    if matches!(attempt, Attempt::Interrupted) {
        delete_interrupted_staging(environment)
            .map_err(|cut| cut_in_place(environment, cut, 5, identity_line))?;
    }
    let checked = check_what_is_there(environment, &attempt)?;
    let verified = read_each_file(environment, request, &attempt, &checked)?;
    restrict_files(environment, &checked)?;
    let record = record_and_stage(environment, request, &attempt, &checked, &verified)
        .map_err(|cut| cut_in_place(environment, cut, 5, identity_line))?;
    commit(environment, record, &checked)?;
    check_data_directories(environment, false)?;
    Ok(Prepared::Committed)
}

/// Item 2.1.
fn close_directories(environment: &Environment<'_>) -> Result<(), Stop> {
    for (path, point) in [
        (&environment.paths.product_root, Point::CloseProductRoot),
        (&environment.paths.data_root, Point::CloseDataRoot),
    ] {
        close(environment, path, point).map_err(|cut| {
            cut_in_place(
                environment,
                naming(path, cut),
                EXIT_SERVICE_LOG,
                stderr_service_log,
            )
        })?;
    }
    Ok(())
}

fn close(environment: &Environment<'_>, path: &Path, point: Point) -> Result<(), Injected> {
    environment.watch.at(point)?;
    let handle = custody::open_waiting(path, SECURITY_ACCESS, SHARE_ALL, environment.wait)?;
    custody::check_shape(&handle, path, Kind::Directory)?;
    check_closable(environment, &custody::security_of(&handle, path)?, path)?;
    custody::set_security(
        handle,
        path,
        &environment.custody.closed_directory_sddl,
        environment.wait,
    )?;
    Ok(())
}

/// Item 2.1's condition: owned by SYSTEM or Administrators, with a DACL
/// that lets no one else change it or take its ownership.
fn check_closable(
    environment: &Environment<'_>,
    descriptor: &Descriptor,
    path: &Path,
) -> Result<(), String> {
    if !descriptor.owned_by(&environment.custody.owners)? {
        return Err(format!(
            "{} is owned by another account; {REPAIR_OWNER}",
            path.display()
        ));
    }
    let entries = descriptor
        .allow_entries(path)
        .map_err(|reason| format!("{reason}; {REPAIR_OWNER}"))?;
    let Some(entries) = entries else {
        return Err(format!("{} has no DACL; {REPAIR_OWNER}", path.display()));
    };
    for entry in entries {
        let trusted = environment
            .custody
            .trusted
            .iter()
            .any(|principal| principal.is(&entry.sid));
        if !trusted && entry.mask & (WRITE_DAC | WRITE_OWNER | GENERIC_ALL) != 0 {
            return Err(format!(
                "{}'s DACL lets another account change it or take its ownership; {REPAIR_OWNER}",
                path.display()
            ));
        }
    }
    Ok(())
}

/// Item 4: a staging directory that counts, without a record, deleted
/// without following a reparse point.
fn delete_interrupted_staging(environment: &Environment<'_>) -> Result<(), Injected> {
    let staging = environment.paths.staging();
    environment
        .watch
        .at(Point::DeleteInterruptedStaging)
        .map_err(|cut| naming(&staging, cut))?;
    let owned = staging.clone();
    custody::bounded(environment.wait, &staging, move || {
        std::fs::remove_dir_all(&owned).map_err(|e| format!("delete {}: {e}", owned.display()))
    })?;
    Ok(())
}

/// What item 2.2 found.
struct Checked {
    files: Vec<(&'static str, custody::FileId)>,
    /// The one SID besides SYSTEM the identity files' DACLs allow.
    enrolling_account: Option<Vec<u8>>,
}

/// The product's name for an entry of the data root.
fn known_name(entry: &str) -> Option<&'static str> {
    REQUIRED
        .iter()
        .chain(std::iter::once(&TEMPORARY))
        .find(|name| name.eq_ignore_ascii_case(entry))
        .copied()
}

/// Item 2.2.
fn check_what_is_there(environment: &Environment<'_>, attempt: &Attempt) -> Result<Checked, Stop> {
    let paths = &environment.paths;
    let stop = |reason: String| in_place(environment, 5, identity_line(&reason));

    check_root_entries(environment, attempt.recorded().is_some()).map_err(stop)?;
    let handle = custody::open_waiting(
        &paths.data_root,
        FILE_READ_ATTRIBUTES,
        SHARE_ALL,
        environment.wait,
    )
    .map_err(stop)?;
    if custody::is_case_sensitive(&handle, &paths.data_root).map_err(stop)? {
        return Err(stop(format!(
            "{} has case-sensitive lookup on; repair: an administrator turns it off",
            paths.data_root.display()
        )));
    }
    drop(handle);

    let mut names = data_root_names(environment).map_err(stop)?;
    for name in REQUIRED {
        if !names.contains(&name) {
            return Err(stop(format!(
                "{} is missing from {}; {REPAIR_ANEW}",
                name,
                paths.data_root.display()
            )));
        }
    }

    // In the order of `REQUIRED`, the temporary last.
    names.sort_by_key(|name| {
        REQUIRED
            .iter()
            .position(|n| n == name)
            .unwrap_or(REQUIRED.len())
    });
    let mut files = Vec::new();
    let mut enrolling_account: Option<Vec<u8>> = None;
    for name in names {
        let path = paths.data_root.join(name);
        environment
            .watch
            .at(Point::Check(name))
            .map_err(|cut| cut_in_place(environment, naming(&path, cut), 5, identity_line))?;
        let (id, others) = check_file(environment, &path, name).map_err(stop)?;
        files.push((name, id));
        if let Some(other) = others {
            if enrolling_account.as_ref().is_some_and(|sid| *sid != other) {
                return Err(stop(format!(
                    "{} allows another account than the other identity files; {REPAIR_ANEW}",
                    paths.data_root.join(name).display()
                )));
            }
            enrolling_account = Some(other);
        }
    }
    Ok(Checked {
        files,
        enrolling_account,
    })
}

/// `<P>` holds no entry but the data root and, when `staging`, the staging
/// directory.
fn check_root_entries(environment: &Environment<'_>, staging: bool) -> Result<(), String> {
    let paths = &environment.paths;
    let data_root_name = file_name(&paths.data_root);
    let staging_name = file_name(&paths.staging());
    let mut seen = HashSet::new();
    for entry in custody::entries(&paths.product_root, environment.wait)? {
        let accepted = entry.eq_ignore_ascii_case(&data_root_name)
            || (staging && entry.eq_ignore_ascii_case(&staging_name));
        if !accepted || !seen.insert(entry.to_ascii_lowercase()) {
            return Err(format!(
                "{} holds {entry}, which is not the agent's; {REPAIR_ENTRY}",
                paths.product_root.display()
            ));
        }
    }
    Ok(())
}

/// The data root's entries, each a name of the console installation's, once.
fn data_root_names(environment: &Environment<'_>) -> Result<Vec<&'static str>, String> {
    let data_root = &environment.paths.data_root;
    let mut names: Vec<&'static str> = Vec::new();
    for entry in custody::entries(data_root, environment.wait)? {
        match known_name(&entry) {
            Some(name) if !names.contains(&name) => names.push(name),
            _ => {
                return Err(format!(
                    "{} holds {entry}, which is not the agent's; {REPAIR_ENTRY}",
                    data_root.display()
                ))
            }
        }
    }
    Ok(names)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// One file of item 2.2, unchanged: a regular file with one link, owned by
/// SYSTEM or Administrators, and an identity file's DACL. It returns the
/// file's ID and the SID besides SYSTEM that an identity file allows.
fn check_file(
    environment: &Environment<'_>,
    path: &Path,
    name: &'static str,
) -> Result<(custody::FileId, Option<Vec<u8>>), String> {
    let handle = custody::open_waiting(path, INSPECT_ACCESS, SHARE_ALL, environment.wait)?;
    let info = custody::file_information(&handle, path)?;
    let temporary = name == TEMPORARY;
    if info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0 {
        let repair = if temporary {
            REPAIR_TEMPORARY
        } else {
            REPAIR_ENTRY
        };
        return Err(format!(
            "{} is not a regular file; {repair}",
            path.display()
        ));
    }
    if info.nNumberOfLinks != 1 {
        let repair = if temporary {
            REPAIR_TEMPORARY
        } else {
            REPAIR_LINK
        };
        return Err(format!("{} is hard-linked; {repair}", path.display()));
    }
    let descriptor = custody::security_of(&handle, path)?;
    if !descriptor.owned_by(&environment.custody.owners)? {
        let repair = if temporary {
            REPAIR_TEMPORARY
        } else {
            REPAIR_OWNER
        };
        return Err(format!(
            "{} is owned by another account; {repair}",
            path.display()
        ));
    }
    let id = custody::file_id(&info);
    if !IDENTITY_FILES.contains(&name) {
        return Ok((id, None));
    }
    let Some(entries) = descriptor.allow_entries(path)? else {
        return Err(format!("{} has no DACL; {REPAIR_ANEW}", path.display()));
    };
    // SYSTEM alone, or SYSTEM and either one other SID or Administrators,
    // as item 2.4 leaves it.
    let service = &environment.custody.service;
    let mut allows_service = false;
    let mut allows_administrators = false;
    let mut others: Vec<Vec<u8>> = Vec::new();
    for entry in entries {
        if service.is(&entry.sid) {
            allows_service = true;
        } else if Principal::Administrators.is(&entry.sid) {
            allows_administrators = true;
        } else if !others.contains(&entry.sid) {
            others.push(entry.sid);
        }
    }
    if !allows_service || others.len() > 1 || (allows_administrators && !others.is_empty()) {
        return Err(format!(
            "{}'s DACL allows more than the service and one account; {REPAIR_ANEW}",
            path.display()
        ));
    }
    Ok((id, others.pop()))
}

/// What item 2.3 read and verified: everything after is written from these
/// bytes.
struct Verified {
    config_text: String,
    certificates: Vec<Vec<u8>>,
    certificate: Vec<u8>,
    sealed_key: Vec<u8>,
    identity: Vec<u8>,
    agent_id: String,
    key_fingerprint: String,
}

/// Item 2.3.
fn read_each_file(
    environment: &Environment<'_>,
    request: &Request,
    attempt: &Attempt,
    checked: &Checked,
) -> Result<Verified, Stop> {
    let paths = &environment.paths;
    let mut bytes: BTreeMap<&'static str, Vec<u8>> = BTreeMap::new();
    for (name, id) in &checked.files {
        if *name == TEMPORARY {
            continue;
        }
        let path = paths.data_root.join(name);
        environment
            .watch
            .at(Point::Read(name))
            .map_err(|cut| cut_in_place(environment, naming(&path, cut), 5, identity_line))?;
        let (content, read_id) = custody::read_unshared(&path, READ_LIMIT, environment.wait)
            .map_err(|reason| in_place(environment, 5, identity_line(&reason)))?;
        if read_id != *id {
            return Err(in_place(
                environment,
                5,
                identity_line(&format!(
                    "{} is not the file that was checked",
                    path.display()
                )),
            ));
        }
        bytes.insert(name, content);
    }
    let take = |bytes: &mut BTreeMap<&'static str, Vec<u8>>, name: &str| {
        bytes.remove(name).unwrap_or_default()
    };

    let config_text = check_legacy_config(environment, take(&mut bytes, CONFIG))
        .map_err(|line| in_place(environment, 2, line))?;
    let certificates = check_anchor(&take(&mut bytes, ANCHOR), &request.fingerprints)
        .map_err(|reason| in_place(environment, 6, anchor_line(&reason)))?;
    let sealed_key = take(&mut bytes, KEY);
    let certificate = take(&mut bytes, CERT);
    let identity = take(&mut bytes, IDENTITY);
    let (agent_id, key_fingerprint) = check_identity(
        environment,
        attempt.recorded(),
        &sealed_key,
        &certificate,
        &identity,
    )
    .map_err(|reason| in_place(environment, 5, identity_line(&reason)))?;
    Ok(Verified {
        config_text,
        certificates,
        certificate,
        sealed_key,
        identity,
        agent_id,
        key_fingerprint,
    })
}

/// A console path is the data root's file of its name, spelled with either
/// slash and equal but for ASCII case.
fn is_console_path(configured: &str, fixed: &Path) -> bool {
    fixed
        .to_str()
        .is_some_and(|fixed| fixed.eq_ignore_ascii_case(&configured.replace('/', "\\")))
}

/// `agent.toml`, read: code 2's line when it is refused.
fn check_legacy_config(environment: &Environment<'_>, bytes: Vec<u8>) -> Result<String, String> {
    let path = &environment.paths.config;
    let invalid = |reason: &str| format!("cg-agent: invalid config {}: {reason}", path.display());
    let text = String::from_utf8(bytes).map_err(|_| invalid("the file is not UTF-8"))?;
    let config = crate::config::load_from_str(&text).map_err(|e| config_error_line(path, &e))?;
    let document: toml::Value = text
        .parse()
        .map_err(|_| invalid("the file is not valid TOML"))?;
    if document.get("setup").is_some() {
        return Err(invalid("a console configuration holds no setup table"));
    }
    let Some(enrollment) = config.enrollment.as_ref() else {
        return Err(invalid("the migration requires [enrollment]"));
    };
    let data_root = &environment.paths.data_root;
    for (key, configured, name) in [
        (
            "server.trust_anchor_path",
            config.server.trust_anchor_path.as_deref().unwrap_or(""),
            ANCHOR,
        ),
        ("enrollment.cert_path", enrollment.cert_path.as_str(), CERT),
        ("enrollment.key_path", enrollment.key_path.as_str(), KEY),
        (
            "enrollment.identity_path",
            enrollment.identity_path.as_str(),
            IDENTITY,
        ),
    ] {
        if !is_console_path(configured, &data_root.join(name)) {
            return Err(invalid(&format!(
                "{key} is not the console layout's path in the data root"
            )));
        }
    }
    if !config
        .server
        .heartbeat_url
        .as_deref()
        .is_some_and(|url| url.starts_with("https://"))
    {
        return Err(invalid(
            "the migration requires an https server.heartbeat_url: add it before migrating",
        ));
    }
    Ok(text)
}

fn fingerprint(der: &[u8]) -> String {
    let mut text = String::with_capacity(7 + 64);
    text.push_str("sha256:");
    for byte in Sha256::digest(der) {
        use std::fmt::Write;
        let _ = write!(text, "{byte:02x}");
    }
    text
}

/// The distinct certificates of `server-ca.pem`, in their order: exactly
/// those of the fingerprints given (§Data contracts §3's exact-set rule).
fn check_anchor(bytes: &[u8], expected: &BTreeSet<String>) -> Result<Vec<Vec<u8>>, String> {
    let mut distinct: Vec<Vec<u8>> = Vec::new();
    for certificate in rustls::pki_types::CertificateDer::pem_slice_iter(bytes) {
        let certificate = certificate.map_err(|_| "server-ca.pem cannot be read".to_string())?;
        if !distinct.iter().any(|der| der == certificate.as_ref()) {
            distinct.push(certificate.as_ref().to_vec());
        }
    }
    let found: BTreeSet<String> = distinct.iter().map(|der| fingerprint(der)).collect();
    if found != *expected {
        return Err(
            "the certificates of server-ca.pem are not exactly those of the fingerprints given"
                .to_string(),
        );
    }
    Ok(distinct)
}

/// `key.dat`, `cert.pem` and `identity.json`, read: the `agent_id` and the
/// key's fingerprint, or code 5's reason.
fn check_identity(
    environment: &Environment<'_>,
    recorded: Option<&Record>,
    sealed: &[u8],
    certificate: &[u8],
    identity: &[u8],
) -> Result<(String, String), String> {
    let persisted: PersistedIdentity = serde_json::from_slice(identity)
        .map_err(|_| format!("{IDENTITY} does not parse; {REPAIR_ANEW}"))?;
    let secret =
        Zeroizing::new(default_store().unseal(sealed).map_err(|_| {
            format!("{KEY} does not unseal with this machine's key; {REPAIR_ANEW}")
        })?);
    if secret.len() != KEY_LEN {
        return Err(format!(
            "{KEY} does not hold an Ed25519 seed; {REPAIR_ANEW}"
        ));
    }
    let mut seed = Zeroizing::new([0u8; KEY_LEN]);
    seed.copy_from_slice(&secret);
    let public_key = AgentKeypair::from_secret_bytes(&seed).public_key_bytes();
    let key_fingerprint = pubkey_fingerprint(&public_key);
    if key_fingerprint != persisted.agent_pubkey_fingerprint {
        return Err(format!(
            "{KEY} is not the key {IDENTITY} names; {REPAIR_ANEW}"
        ));
    }
    if let Some(record) = recorded {
        if record.key_fingerprint != key_fingerprint || record.agent_id != persisted.agent_id {
            return Err(format!(
                "the identity is not the one the migration recorded; {REPAIR_ANEW}"
            ));
        }
    }
    let text = std::str::from_utf8(certificate)
        .map_err(|_| format!("{CERT} is not a PEM document; {REPAIR_ANEW}"))?;
    let certificates =
        crate::x509::certificates_in(text).map_err(|e| format!("{CERT}: {e}; {REPAIR_ANEW}"))?;
    if certificates.len() != 1 {
        return Err(format!(
            "{CERT} does not hold one certificate; {REPAIR_ANEW}"
        ));
    }
    check_consistency(text, &public_key, &persisted.agent_id)
        .map_err(|e| format!("{CERT}: {e}; {REPAIR_ANEW}"))?;
    let expires_at = DateTime::parse_from_rfc3339(&persisted.expires_at)
        .map_err(|_| format!("{IDENTITY}'s expires_at does not parse; {REPAIR_ANEW}"))?;
    if expires_at <= environment.now {
        return Err(format!("the identity has expired; {REPAIR_ANEW}"));
    }
    Ok((persisted.agent_id, key_fingerprint))
}

/// Item 2.4.
fn restrict_files(environment: &Environment<'_>, checked: &Checked) -> Result<(), Stop> {
    for (name, id) in &checked.files {
        restrict_file(environment, name, *id).map_err(|cut| {
            let path = environment.paths.data_root.join(name);
            cut_in_place(environment, naming(&path, cut), 5, identity_line)
        })?;
    }
    Ok(())
}

fn restrict_file(
    environment: &Environment<'_>,
    name: &'static str,
    id: custody::FileId,
) -> Result<(), Injected> {
    let path = environment.paths.data_root.join(name);
    environment.watch.at(Point::Restrict(name))?;
    let handle = custody::open_waiting(&path, SECURITY_ACCESS, SHARE_ALL, environment.wait)?;
    let check_links = |handle: &custody::Handle| -> Result<custody::FileId, String> {
        let info = custody::file_information(handle, &path)?;
        if info.nNumberOfLinks != 1 {
            return Err(format!("{} is hard-linked; {REPAIR_LINK}", path.display()));
        }
        Ok(custody::file_id(&info))
    };
    if check_links(&handle)? != id {
        return Err(format!("{} is not the file that was checked", path.display()).into());
    }
    let handle = custody::set_security(
        handle,
        &path,
        &environment.custody.shared_file_sddl,
        environment.wait,
    )?;
    environment.watch.at(Point::Restricted(name))?;
    check_links(&handle)?;
    Ok(())
}

/// Item 2.5.
fn record_and_stage(
    environment: &Environment<'_>,
    request: &Request,
    attempt: &Attempt,
    checked: &Checked,
    verified: &Verified,
) -> Result<Record, Injected> {
    let custody_of = &environment.custody;
    let staging = environment.paths.staging();
    let record_path = environment.staged_record();
    let enrolling_account = match &checked.enrolling_account {
        Some(sid) => Some(custody::sid_text(sid)?),
        None => attempt
            .recorded()
            .and_then(|record| record.enrolling_account.clone()),
    };
    let record = Record {
        choice: request.choice,
        fingerprints: request.fingerprints.iter().cloned().collect(),
        agent_id: verified.agent_id.clone(),
        key_fingerprint: verified.key_fingerprint.clone(),
        enrolling_account,
        files: checked
            .files
            .iter()
            .map(|(name, id)| ((*name).to_string(), id.to_string()))
            .collect(),
        reversal_pending: false,
        copies_not_reached: Vec::new(),
    };

    if attempt.recorded().is_some() {
        environment.watch.at(Point::WriteRecord)?;
        rewrite(environment, &record_path, &record)?;
        clear_staged(environment)?;
    } else {
        environment.watch.at(Point::CreateStaging)?;
        custody::create_new_directory(&staging, &custody_of.closed_directory_sddl)?;
        environment.watch.at(Point::WriteRecord)?;
        let bytes = serde_json::to_vec_pretty(&record)
            .map_err(|e| format!("write {}: {e}", record_path.display()))?;
        custody::create_new_file(&record_path, &custody_of.shared_file_sddl, &bytes)?;
    }

    let shared = &custody_of.shared_file_sddl;
    let config = generated_config(environment, &verified.config_text)?;
    let anchor = anchor_pem(&verified.certificates);
    environment.watch.at(Point::Stage(CONFIG))?;
    custody::create_new_file(&staging.join(CONFIG), shared, config.as_bytes())?;
    environment.watch.at(Point::Stage(ANCHOR))?;
    custody::create_new_file(&staging.join(ANCHOR), shared, anchor.as_bytes())?;
    let identity_dir = staging.join(IDENTITY_DIRECTORY);
    environment.watch.at(Point::Stage(IDENTITY_DIRECTORY))?;
    custody::create_new_directory(&identity_dir, &custody_of.identity_directory_sddl)?;
    for (name, bytes) in [
        (CERT, &verified.certificate),
        (KEY, &verified.sealed_key),
        (IDENTITY, &verified.identity),
    ] {
        environment.watch.at(Point::Stage(name))?;
        custody::create_new_file(
            &identity_dir.join(name),
            &custody_of.identity_file_sddl,
            bytes,
        )?;
    }
    environment.watch.at(Point::CheckStaged)?;
    check_staged(environment, &record)?;
    Ok(record)
}

/// A continued migration replaces the staged files: all but its record.
fn clear_staged(environment: &Environment<'_>) -> Result<(), String> {
    let staging = environment.paths.staging();
    for entry in custody::entries(&staging, environment.wait)? {
        if entry.eq_ignore_ascii_case(RECORD) {
            continue;
        }
        let path = staging.join(&entry);
        let owned = path.clone();
        custody::bounded(environment.wait, &path, move || {
            let metadata = std::fs::symlink_metadata(&owned)
                .map_err(|e| format!("inspect {}: {e}", owned.display()))?;
            let removed = if metadata.is_dir() {
                std::fs::remove_dir_all(&owned)
            } else {
                std::fs::remove_file(&owned)
            };
            removed.map_err(|e| format!("delete {}: {e}", owned.display()))
        })?;
    }
    Ok(())
}

/// The installed `agent.toml`, from the legacy one: the fixed paths, the
/// legacy `agent.id`, the hostname Windows reports and no token.
fn generated_config(environment: &Environment<'_>, legacy: &str) -> Result<String, String> {
    let paths = &environment.paths;
    let mut document: toml::Value = legacy
        .parse()
        .map_err(|_| "the legacy configuration is not valid TOML".to_string())?;
    let hostname = custody::hostname()?;
    let path = |path: &Path| toml::Value::String(path.display().to_string());
    table_of(&mut document, "server")?
        .insert("trust_anchor_path".to_string(), path(&paths.trust_anchor));
    table_of(&mut document, "agent")?.insert("hostname".to_string(), toml::Value::String(hostname));
    let enrollment = table_of(&mut document, "enrollment")?;
    enrollment.remove("token");
    enrollment.insert("cert_path".to_string(), path(&paths.cert));
    enrollment.insert("key_path".to_string(), path(&paths.key));
    enrollment.insert("identity_path".to_string(), path(&paths.identity));
    toml::to_string(&document).map_err(|_| "the configuration cannot be written".to_string())
}

fn table_of<'a>(
    document: &'a mut toml::Value,
    name: &str,
) -> Result<&'a mut toml::map::Map<String, toml::Value>, String> {
    document
        .get_mut(name)
        .and_then(toml::Value::as_table_mut)
        .ok_or_else(|| format!("the legacy configuration lacks [{name}]"))
}

/// `server-ca.pem`, from the verified certificates.
fn anchor_pem(certificates: &[Vec<u8>]) -> String {
    let blocks: Vec<pem::Pem> = certificates
        .iter()
        .map(|der| pem::Pem::new("CERTIFICATE", der.clone()))
        .collect();
    pem::encode_many_config(
        &blocks,
        pem::EncodeConfig::new().set_line_ending(pem::LineEnding::LF),
    )
}

/// The staged tree, as a start checks the data root (item 2.5), and its
/// identity, which loads with the record's `agent_id` and key.
fn check_staged(environment: &Environment<'_>, record: &Record) -> Result<(), String> {
    let staging = environment.paths.staging();
    let configuration = &environment.custody.configuration;
    let identity_policy = &environment.custody.identity;
    custody::check(&staging, Kind::Directory, configuration)?;
    custody::check(&environment.staged_record(), Kind::File, configuration)?;
    let config_path = staging.join(CONFIG);
    let bytes = custody::read_checked(&config_path, configuration)?
        .ok_or_else(|| format!("{} does not exist", config_path.display()))?;
    let text =
        String::from_utf8(bytes).map_err(|_| format!("{} is not UTF-8", config_path.display()))?;
    let mut config: AgentConfig =
        crate::config::load_from_str(&text).map_err(|e| config_error_line(&config_path, &e))?;
    check_installed_config(&environment.paths, &config_path, &config)?;
    let anchor_path = staging.join(ANCHOR);
    let anchor = custody::read_checked(&anchor_path, configuration)?
        .ok_or_else(|| format!("{} does not exist", anchor_path.display()))?;
    let expected: BTreeSet<String> = record.fingerprints.iter().cloned().collect();
    check_anchor(&anchor, &expected)?;

    let identity_dir = staging.join(IDENTITY_DIRECTORY);
    custody::check(&identity_dir, Kind::Directory, identity_policy)?;
    for name in IDENTITY_FILES {
        custody::check(&identity_dir.join(name), Kind::File, identity_policy)?;
    }
    let enrollment = config.enrollment.as_mut().ok_or("no [enrollment]")?;
    enrollment.cert_path = identity_dir.join(CERT).display().to_string();
    enrollment.key_path = identity_dir.join(KEY).display().to_string();
    enrollment.identity_path = identity_dir.join(IDENTITY).display().to_string();
    let identity = installed::load(&config).map_err(|e| e.to_string())?;
    if identity.agent_id != record.agent_id
        || pubkey_fingerprint(&identity.keypair.public_key_bytes()) != record.key_fingerprint
    {
        return Err("the staged identity is not the recorded one".to_string());
    }
    Ok(())
}

/// What item 2.2 would refuse of the data root just before the first
/// rename: an entry created in a file transaction before item 2.1 can be
/// committed past the closed directories' DACLs (Amendment 2026-11-07,
/// second).
fn check_names_again(environment: &Environment<'_>, checked: &Checked) -> Result<(), String> {
    check_root_entries(environment, true)?;
    let data_root = &environment.paths.data_root;
    let names = data_root_names(environment)?;
    for name in &names {
        if !checked.files.iter().any(|(known, _)| known == name) {
            let repair = if *name == TEMPORARY {
                REPAIR_TEMPORARY
            } else {
                REPAIR_ENTRY
            };
            return Err(format!(
                "{} appeared after the check; {repair}",
                data_root.join(name).display()
            ));
        }
    }
    for name in REQUIRED {
        if !names.contains(&name) {
            return Err(format!(
                "{} is missing from {}; {REPAIR_ANEW}",
                name,
                data_root.display()
            ));
        }
    }
    Ok(())
}

/// Item 2.6.
fn commit(
    environment: &Environment<'_>,
    mut record: Record,
    checked: &Checked,
) -> Result<(), Stop> {
    let paths = &environment.paths;
    (|| -> Result<(), Injected> {
        environment.watch.at(Point::FirstRename)?;
        check_names_again(environment, checked)?;
        custody::rename(&paths.data_root, &paths.legacy(), environment.wait)?;
        Ok(())
    })()
    .map_err(|cut| cut_in_place(environment, cut, 5, identity_line))?;

    let second = (|| -> Result<(), Injected> {
        environment.watch.at(Point::SecondRename)?;
        custody::rename(&paths.staging(), &paths.data_root, environment.wait)?;
        Ok(())
    })();
    let cause = match second {
        Ok(()) => return Ok(()),
        Err(Injected::Crash) => return Err(Stop::Crashed),
        Err(Injected::Failure(reason)) => reason,
    };
    record.reversal_pending = true;
    let marked = (|| -> Result<(), Injected> {
        environment.watch.at(Point::RecordReversal)?;
        rewrite(environment, &environment.staged_record(), &record)?;
        Ok(())
    })();
    match marked {
        Ok(()) => Err(reverse(
            environment,
            record,
            &format!("the commit failed ({cause})"),
        )),
        Err(Injected::Crash) => Err(Stop::Crashed),
        Err(Injected::Failure(reason)) => Err(both_directories(
            environment,
            &format!("the commit failed ({cause}), and its reversal could not be recorded ({reason}); the next start completes it"),
        )),
    }
}

/// What one pass over the old copies did (item 5). Names are the entries
/// of the legacy directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Copies {
    pub restricted: Vec<String>,
    pub removed: Vec<String>,
    pub not_reached: Vec<String>,
    /// The legacy directory is gone.
    pub directory_removed: bool,
}

/// Item 5, after `RUNNING`. A copy that cannot be reached does not stop the
/// start; it is logged, named in the record and retried at the next start.
/// Item 7's log line says what happened. The error is why the pass could
/// not run at all, logged by the caller.
pub fn copies(environment: &Environment<'_>) -> Result<Copies, String> {
    let paths = &environment.paths;
    let mut record = data_root_record(environment)
        .map_err(|_| "the record cannot be read".to_string())?
        .ok_or("the data root holds no record")?;
    let legacy = paths.legacy();
    let mut report = Copies::default();
    if !counts(environment, &legacy) {
        return Ok(report);
    }
    let not_reached = |report: &mut Copies, name: String, reason: &str| {
        tracing::warn!(copy = %name, reason, "a copy of the console installation was not reached");
        report.not_reached.push(name);
    };

    let directory_restricted = match restrict_legacy_directory(environment, &legacy) {
        Ok(()) => true,
        Err(Injected::Crash) => return Err("the pass was cut short".to_string()),
        Err(Injected::Failure(reason)) => {
            not_reached(&mut report, file_name(&legacy), &reason);
            false
        }
    };
    let mut names = custody::entries(&legacy, environment.wait)?;
    names.sort();
    for name in names {
        let path = legacy.join(&name);
        match restrict_copy(environment, &path, &name, &record) {
            Ok(()) => report.restricted.push(name.clone()),
            Err(Injected::Crash) => return Err("the pass was cut short".to_string()),
            Err(Injected::Failure(reason)) => {
                not_reached(&mut report, name, &reason);
                continue;
            }
        }
        if record.choice != Choice::Remove {
            continue;
        }
        match remove_copy(environment, &path, &name, &record) {
            Ok(()) => report.removed.push(name),
            Err(reason) => not_reached(&mut report, name, &reason),
        }
    }
    if directory_restricted
        && record.choice == Choice::Remove
        && custody::entries(&legacy, environment.wait)?.is_empty()
    {
        match remove_legacy_directory(environment, &legacy) {
            Ok(()) => report.directory_removed = true,
            Err(Injected::Crash) => return Err("the pass was cut short".to_string()),
            Err(Injected::Failure(reason)) => {
                not_reached(&mut report, file_name(&legacy), &reason);
            }
        }
    }

    if record.copies_not_reached != report.not_reached {
        record.copies_not_reached = report.not_reached.clone();
        let rewritten = (|| -> Result<(), Injected> {
            environment.watch.at(Point::RewriteRecord)?;
            rewrite(environment, &paths.record(), &record)?;
            Ok(())
        })();
        if let Err(Injected::Failure(reason)) = rewritten {
            tracing::warn!(reason, "the migration's record could not be rewritten");
        }
    }
    tracing::info!(
        component = "migration",
        agent.id = %record.agent_id,
        key_fingerprint = %record.key_fingerprint,
        choice = match record.choice {
            Choice::Remove => "remove",
            Choice::Keep => "keep",
        },
        copies_restricted = report.restricted.len(),
        copies_removed = report.removed.len(),
        copies_not_reached = report.not_reached.len(),
        "the host was migrated from the console installation"
    );
    Ok(report)
}

fn restrict_legacy_directory(environment: &Environment<'_>, legacy: &Path) -> Result<(), Injected> {
    environment.watch.at(Point::RestrictLegacyDirectory)?;
    let handle = custody::open_waiting(legacy, SECURITY_ACCESS, SHARE_ALL, environment.wait)?;
    custody::check_shape(&handle, legacy, Kind::Directory)?;
    custody::set_security(
        handle,
        legacy,
        &environment.custody.identity_directory_sddl,
        environment.wait,
    )?;
    Ok(())
}

/// What the handle's object must be for item 5 to touch it: a regular file
/// with one link, with the file ID the record holds.
fn check_copy(
    handle: &custody::Handle,
    path: &Path,
    name: &str,
    record: &Record,
) -> Result<(), String> {
    let info = custody::file_information(handle, path)?;
    if info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0 {
        return Err(format!("{} is not a regular file", path.display()));
    }
    let id = custody::file_id(&info).to_string();
    let expected = known_name(name).and_then(|known| record.files.get(known));
    if expected != Some(&id) {
        return Err(format!(
            "{} is not the file the migration checked",
            path.display()
        ));
    }
    if info.nNumberOfLinks != 1 {
        return Err(format!("{} is hard-linked", path.display()));
    }
    Ok(())
}

/// One object of the legacy directory, restricted through a handle that
/// does not ask to delete: a reader that does not share deletion stops the
/// removal, not this.
fn restrict_copy(
    environment: &Environment<'_>,
    path: &Path,
    name: &str,
    record: &Record,
) -> Result<(), Injected> {
    let Some(known) = known_name(name) else {
        return Err(format!("{} is not an object the migration checked", path.display()).into());
    };
    environment.watch.at(Point::Copy(known))?;
    let handle = custody::open_waiting(path, SECURITY_ACCESS, SHARE_ALL, environment.wait)?;
    check_copy(&handle, path, name, record)?;
    let sddl = if IDENTITY_FILES.contains(&known) {
        &environment.custody.identity_file_sddl
    } else {
        &environment.custody.shared_file_sddl
    };
    custody::set_security(handle, path, sddl, environment.wait)?;
    Ok(())
}

/// With `remove`, the restricted object is deleted through a handle of its
/// own, checked again.
fn remove_copy(
    environment: &Environment<'_>,
    path: &Path,
    name: &str,
    record: &Record,
) -> Result<(), String> {
    let handle = custody::open_waiting(
        path,
        DELETE | FILE_READ_ATTRIBUTES,
        SHARE_ALL,
        environment.wait,
    )?;
    check_copy(&handle, path, name, record)?;
    custody::delete_through(handle, path, environment.wait)
}

fn remove_legacy_directory(environment: &Environment<'_>, legacy: &Path) -> Result<(), Injected> {
    environment.watch.at(Point::RemoveLegacyDirectory)?;
    let handle = custody::open_waiting(
        legacy,
        DELETE | FILE_READ_ATTRIBUTES,
        SHARE_ALL,
        environment.wait,
    )?;
    custody::check_shape(&handle, legacy, Kind::Directory)?;
    custody::delete_through(handle, legacy, environment.wait)?;
    Ok(())
}
