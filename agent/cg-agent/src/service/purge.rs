//! The purge (SPEC-024 Amendment 2026-11-15, points 4 to 7): a start of the
//! service, as SYSTEM, asked by the single start argument `--purge`, removes
//! the product's data during `START_PENDING` and never reports `RUNNING`.
//!
//! It takes the instance lock, then opens `<P>` and the data root without
//! following a reparse point, checks them as the file log does and holds each
//! open, sharing no deletion, until it removes it; it reads the host's state by
//! name (`layout::classify`) and refuses an untrusted state, a migration in
//! progress or a console layout before anything changes. It removes a closed
//! list of names, nothing it reads from the configuration: each file through a
//! handle that follows no reparse point and is checked before the deletion it
//! carries, each directory once a listing read just before shows it empty.
//! The order keeps every state it leaves one a start goes on from: the
//! configuration first, the staged file before it; then the legacy directory;
//! then the identity, each of these three steps only once the one before left
//! nothing; then the rest, going on past what it leaves. Everything takes the
//! migration's `Environment`, so that the tests point it at a temporary root
//! and inject a failure and a crash at each `Point`.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::time::{Duration, Instant};

use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
    READ_CONTROL,
};

use super::layout::{classify, HostState};
use super::migration::{refused, Environment, Injected, Point, Stop, SHARE_ALL};
use super::{EXIT_PURGE_LEFT, EXIT_PURGE_REFUSED, EXIT_SERVICE_LOG};
use crate::custody::{self, Handle, Kind, Policy};

/// The purge begins no removal this long after it took the lock (point 4).
pub const BOUND: Duration = Duration::from_secs(120);

/// The line of a purge start that ended abnormally (point 4).
pub const ABNORMAL: &str = "cg-agent: the purge ended abnormally";

/// The most names code 15's line lists before it gives their number (point 7).
const NAMED: usize = 20;

/// The directories the purge works in (point 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    ProductRoot,
    DataRoot,
    Legacy,
    Identity,
    Logs,
}

/// Point 6's first step: the staged configuration before `agent.toml`.
const CONFIGURATION: [&str; 3] = ["agent.toml.setup", "agent.toml", "agent.toml.hygiene-tmp"];
/// The legacy directory's names (the Amendment 2026-11-05, item 2.2), its
/// `agent.toml` first.
const LEGACY: [&str; 6] = [
    "agent.toml",
    "server-ca.pem",
    "cert.pem",
    "key.dat",
    "identity.json",
    "agent.toml.hygiene-tmp",
];
const IDENTITY: [&str; 3] = ["cert.pem", "key.dat", "identity.json"];
/// The data root's other names.
const REST: [&str; 5] = [
    "server-ca.pem",
    "server-ca.pem.setup",
    "setup-attempt.json",
    "migration.json",
    "migration.json.rewrite-tmp",
];
const LOGS: [&str; 7] = [
    "agent.jsonl",
    "agent.1.jsonl",
    "agent.2.jsonl",
    "agent.3.jsonl",
    "agent.4.jsonl",
    "agent.5.jsonl",
    "agent.rotating.jsonl",
];
const IDENTITY_DIRECTORY: &str = "identity";
const LOGS_DIRECTORY: &str = "logs";

/// What a directory is held open with: listing binds its share mode, which
/// admits readers and writers and no deletion or rename of the directory.
const HOLD_ACCESS: u32 = FILE_LIST_DIRECTORY | READ_CONTROL | FILE_READ_ATTRIBUTES;
const HOLD_SHARE: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE;
/// What a removal opens an object with.
const REMOVE_ACCESS: u32 = DELETE | READ_CONTROL | FILE_READ_ATTRIBUTES;

/// Code 13's line (point 4).
fn directories_line(reason: &str) -> String {
    format!("cg-agent: the purge's directories fail their check: {reason}")
}

/// Code 14's line, naming the state.
fn refused_line(state: &str) -> String {
    format!("cg-agent: the purge refuses this host: {state}; nothing was removed")
}

/// Code 15's line: what was left, at most `NAMED` names and then their
/// number (point 7). Each item names its path.
fn left_line(left: &[String]) -> String {
    let shown = left
        .iter()
        .take(NAMED)
        .cloned()
        .collect::<Vec<_>>()
        .join("; ");
    let more = left.len().saturating_sub(NAMED);
    let count = if left.len() == 1 {
        "1 name".to_string()
    } else {
        format!("{} names", left.len())
    };
    if more == 0 {
        format!("cg-agent: the purge left {count}: {shown}")
    } else {
        format!("cg-agent: the purge left {count}: {shown}; and {more} more")
    }
}

/// A reason that names `path`, whatever the operation's own reason says.
fn naming(path: &Path, reason: &str) -> String {
    if reason.contains(&path.display().to_string()) {
        reason.to_string()
    } else {
        format!("{}: {reason}", path.display())
    }
}

/// The purge start (point 4) with its abnormal ends as code 15: a panic, and a
/// crash a test injects. `lock` takes the instance lock; it is called once,
/// before anything else.
pub fn start(
    environment: &Environment<'_>,
    bound: Duration,
    lock: &mut dyn FnMut() -> Result<(), Stop>,
) -> Result<(), Stop> {
    match catch_unwind(AssertUnwindSafe(|| run(environment, bound, lock))) {
        Ok(Err(Stop::Crashed)) | Err(_) => Err(refused(EXIT_PURGE_LEFT, ABNORMAL.to_string())),
        Ok(result) => result,
    }
}

/// Points 4 to 6. `Ok` when nothing is left; a crash a test injects ends it
/// where it is, with nothing after that point run.
pub fn run(
    environment: &Environment<'_>,
    bound: Duration,
    lock: &mut dyn FnMut() -> Result<(), Stop>,
) -> Result<(), Stop> {
    lock()?;
    let deadline = Instant::now() + bound;
    let paths = &environment.paths;
    let policy = Policy {
        owners: environment.custody.owners.clone(),
        allowed: environment.custody.configuration.allowed.clone(),
        protected: false,
    };

    let product_root = hold_root(environment, &paths.product_root, Area::ProductRoot, &policy)?;
    let data_root = match product_root {
        Some(_) => hold_root(environment, &paths.data_root, Area::DataRoot, &policy)?,
        None => None,
    };

    environment
        .watch
        .at(Point::PurgeState)
        .map_err(|cut| match cut {
            Injected::Crash => Stop::Crashed,
            Injected::Failure(reason) => refused(
                EXIT_PURGE_REFUSED,
                refused_line(&format!("an untrusted state ({reason})")),
            ),
        })?;
    let refusal = match classify(paths, &policy, environment.wait) {
        HostState::Untrusted(reason) => Some(format!("an untrusted state ({reason})")),
        HostState::MigrationInProgress => Some("a migration in progress".to_string()),
        HostState::ConsoleLayout => Some("a console layout".to_string()),
        HostState::Enrolled | HostState::Pending | HostState::Fresh => None,
    };
    if let Some(state) = refusal {
        return Err(refused(EXIT_PURGE_REFUSED, refused_line(&state)));
    }
    let Some(product_root) = product_root else {
        return Ok(());
    };

    let mut purge = Purge {
        environment,
        policy,
        deadline,
        late: false,
        left: Vec::new(),
    };
    purge.walk(product_root, data_root)?;
    if purge.left.is_empty() {
        Ok(())
    } else {
        Err(refused(EXIT_PURGE_LEFT, left_line(&purge.left)))
    }
}

/// Open `<P>` or the data root, if there, without following a reparse point,
/// check it as the file log does, and hold it (code 13).
fn hold_root(
    environment: &Environment<'_>,
    path: &Path,
    area: Area,
    policy: &Policy,
) -> Result<Option<Handle>, Stop> {
    let held = (|| -> Result<Option<Handle>, Injected> {
        environment.watch.at(Point::PurgeHold(area))?;
        let Some(handle) = custody::open_existing(path, HOLD_ACCESS, HOLD_SHARE, environment.wait)?
        else {
            return Ok(None);
        };
        custody::check_shape(&handle, path, Kind::Directory)?;
        custody::check_security(&handle, path, policy)?;
        Ok(Some(handle))
    })();
    held.map_err(|cut| match cut {
        Injected::Crash => Stop::Crashed,
        Injected::Failure(reason) => {
            refused(EXIT_SERVICE_LOG, directories_line(&naming(path, &reason)))
        }
    })
}

/// The entries of `listing` equal to `name` but for ASCII case.
fn matching<'a>(listing: &'a [String], name: &'a str) -> impl Iterator<Item = &'a String> + 'a {
    listing
        .iter()
        .filter(move |entry| entry.eq_ignore_ascii_case(name))
}

fn known(names: &[&str], entry: &str) -> bool {
    names.iter().any(|name| name.eq_ignore_ascii_case(entry))
}

/// One run of the removals.
struct Purge<'e, 'w> {
    environment: &'e Environment<'w>,
    /// The directories' check: `<P>`'s, as the file log checks it.
    policy: Policy,
    deadline: Instant,
    /// The bound has passed: nothing more is removed, and what remains is
    /// named as not reached.
    late: bool,
    /// What was left, each naming its path.
    left: Vec<String>,
}

/// Whether a step of point 6 left nothing; the error is a crash a test
/// injected.
type Step = Result<bool, Stop>;

impl Purge<'_, '_> {
    fn leave(&mut self, path: &Path, reason: &str) {
        self.left.push(naming(path, reason));
    }

    /// Whether the bound has passed; from then on `path` and what follows are
    /// named as not reached.
    fn past_bound(&mut self, path: &Path) -> bool {
        if !self.late && Instant::now() >= self.deadline {
            self.late = true;
        }
        if self.late {
            self.left.push(format!(
                "{} was not reached within the purge's time bound",
                path.display()
            ));
        }
        self.late
    }

    /// A step of the first three ends the walk when it left something and
    /// the bound has not passed; past it, the walk goes on naming.
    fn ends(&self, gone: bool) -> bool {
        !gone && !self.late
    }

    fn walk(&mut self, product_root: Handle, data_root: Option<Handle>) -> Result<(), Stop> {
        let paths = &self.environment.paths;
        let wait = self.environment.wait;
        let root_entries = match custody::entries(&paths.product_root, wait) {
            Ok(entries) => entries,
            Err(reason) => {
                self.leave(&paths.product_root, &reason);
                return Ok(());
            }
        };
        let data_root_name = file_name(&paths.data_root);
        let legacy_name = file_name(&paths.legacy());

        // Past the first three steps the purge goes on past what it leaves,
        // naming it, and names what it does not reach.
        let mut emptied = true;
        if let Some(data_root) = data_root {
            let entries = match custody::entries(&paths.data_root, wait) {
                Ok(entries) => entries,
                Err(reason) => {
                    self.leave(&paths.data_root, &reason);
                    return Ok(());
                }
            };
            if !self.configuration(&entries)? {
                return Ok(());
            }
            for entry in matching(&root_entries, &legacy_name) {
                let gone = self.directory(Area::Legacy, &paths.product_root.join(entry))?;
                if self.ends(gone) {
                    return Ok(());
                }
            }
            for entry in matching(&entries, IDENTITY_DIRECTORY) {
                let gone = self.directory(Area::Identity, &paths.data_root.join(entry))?;
                if self.ends(gone) {
                    return Ok(());
                }
            }
            for name in REST {
                for entry in matching(&entries, name) {
                    emptied &= self.file(Area::DataRoot, name, &paths.data_root.join(entry))?;
                }
            }
            for entry in matching(&entries, LOGS_DIRECTORY) {
                emptied &= self.directory(Area::Logs, &paths.data_root.join(entry))?;
            }
            for entry in &entries {
                let product = known(&CONFIGURATION, entry)
                    || known(&REST, entry)
                    || known(&[IDENTITY_DIRECTORY, LOGS_DIRECTORY], entry);
                if !product {
                    self.leave(&paths.data_root.join(entry), "not a name of the product's");
                    emptied = false;
                }
            }
            drop(data_root);
            // Past the bound, a directory left is named as not reached.
            if emptied || self.late {
                emptied &= self.remove_directory(Area::DataRoot, &paths.data_root)?;
            }
        }
        for entry in &root_entries {
            let product = entry.eq_ignore_ascii_case(&data_root_name)
                || entry.eq_ignore_ascii_case(&legacy_name);
            if !product {
                self.leave(
                    &paths.product_root.join(entry),
                    "not a name of the product's",
                );
                emptied = false;
            }
        }
        drop(product_root);
        if emptied || self.late {
            self.remove_directory(Area::ProductRoot, &paths.product_root)?;
        }
        Ok(())
    }

    /// Point 6's first step: each name only once the one before is gone.
    fn configuration(&mut self, entries: &[String]) -> Step {
        let data_root = self.environment.paths.data_root.clone();
        for name in CONFIGURATION {
            for entry in matching(entries, name) {
                let gone = self.file(Area::DataRoot, name, &data_root.join(entry))?;
                if self.ends(gone) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// A directory of point 5 below the data root or beside it: held while
    /// its files are removed, then removed. The legacy directory, identity's
    /// and its own steps, ends at the first name it leaves.
    fn directory(&mut self, area: Area, path: &Path) -> Step {
        if self.past_bound(path) {
            return Ok(false);
        }
        // The legacy directory is checked as the migration counts it, under
        // `agent.toml`'s policy (the Amendment 2026-11-05, item 4).
        let (names, policy, strict): (&[&'static str], Policy, bool) = match area {
            Area::Legacy => (
                &LEGACY,
                self.environment.custody.configuration.clone(),
                true,
            ),
            Area::Identity => (&IDENTITY, self.policy.clone(), true),
            _ => (&LOGS, self.policy.clone(), false),
        };
        let held = (|| -> Result<(Handle, Vec<String>), Injected> {
            self.environment.watch.at(Point::PurgeHold(area))?;
            let handle =
                custody::open_waiting(path, HOLD_ACCESS, HOLD_SHARE, self.environment.wait)?;
            custody::check_shape(&handle, path, Kind::Directory)?;
            custody::check_security(&handle, path, &policy)?;
            let entries = custody::entries(path, self.environment.wait)?;
            Ok((handle, entries))
        })();
        let (hold, entries) = match held {
            Ok(held) => held,
            Err(Injected::Crash) => return Err(Stop::Crashed),
            Err(Injected::Failure(reason)) => {
                self.leave(path, &reason);
                return Ok(false);
            }
        };
        let mut emptied = true;
        for name in names {
            for entry in matching(&entries, name) {
                let gone = self.file(area, name, &path.join(entry))?;
                emptied &= gone;
                if strict && self.ends(gone) {
                    return Ok(false);
                }
            }
        }
        for entry in entries.iter().filter(|entry| !known(names, entry)) {
            self.leave(&path.join(entry), "not a name of the product's");
            emptied = false;
        }
        drop(hold);
        if !emptied {
            return Ok(false);
        }
        self.remove_directory(area, path)
    }

    /// One file of point 5: opened without following a reparse point,
    /// checked through that handle, deleted through it.
    fn file(&mut self, area: Area, name: &'static str, path: &Path) -> Step {
        if self.past_bound(path) {
            return Ok(false);
        }
        let removed = (|| -> Result<(), Injected> {
            self.environment.watch.at(Point::PurgeFile(area, name))?;
            let wait = self.environment.wait;
            let Some(handle) = custody::open_existing(path, REMOVE_ACCESS, SHARE_ALL, wait)? else {
                return Ok(());
            };
            custody::check_shape(&handle, path, Kind::File)?;
            if !custody::security_of(&handle, path)?.owned_by(&self.environment.custody.owners)? {
                return Err(format!("{} is owned by another account", path.display()).into());
            }
            custody::delete_through(handle, path, wait)?;
            Ok(())
        })();
        match removed {
            Ok(()) => Ok(true),
            Err(Injected::Crash) => Err(Stop::Crashed),
            Err(Injected::Failure(reason)) => {
                self.leave(path, &reason);
                Ok(false)
            }
        }
    }

    /// A directory of point 5, through a handle opened after its hold is
    /// closed and checked as the hold was, once a listing read just before
    /// shows it empty.
    fn remove_directory(&mut self, area: Area, path: &Path) -> Step {
        if self.past_bound(path) {
            return Ok(false);
        }
        let policy = match area {
            Area::Legacy => self.environment.custody.configuration.clone(),
            _ => self.policy.clone(),
        };
        let removed = (|| -> Result<Vec<String>, Injected> {
            self.environment.watch.at(Point::PurgeDirectory(area))?;
            let wait = self.environment.wait;
            let handle = custody::open_waiting(path, REMOVE_ACCESS, SHARE_ALL, wait)?;
            custody::check_shape(&handle, path, Kind::Directory)?;
            custody::check_security(&handle, path, &policy)?;
            let entries = custody::entries(path, wait)?;
            if entries.is_empty() {
                custody::delete_through(handle, path, wait)?;
            }
            Ok(entries)
        })();
        match removed {
            Ok(entries) if entries.is_empty() => Ok(true),
            Ok(entries) => {
                for entry in entries {
                    self.leave(
                        &path.join(entry),
                        "it appeared after the directory was listed",
                    );
                }
                Ok(false)
            }
            Err(Injected::Crash) => Err(Stop::Crashed),
            Err(Injected::Failure(reason)) => {
                self.leave(path, &reason);
                Ok(false)
            }
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}
