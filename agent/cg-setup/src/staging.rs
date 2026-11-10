//! Preflight's staging and its reversal (SPEC-024 Amendment 2026-11-10,
//! items 5 and 7, within item 3 of the Amendment 2026-11-09).
//!
//! `<P>` and the data root are held open from before anything under `<P>` is
//! removed or created until the attempt ends: opened without following a
//! reparse point, sharing no deletion, so that neither can be renamed,
//! deleted or replaced meanwhile, and checked through those handles with the
//! service's check of them. Every name under them is then reached through
//! directories that are the ones checked. A directory the setup creates is
//! created new, with its final security. Each name is deleted through a
//! handle opened without following it, and a directory under a file's name is
//! never deleted.
//!
//! The order is the directories, `setup-attempt.json`, the staged anchor and
//! the staged configuration; a removal takes the staged files, the record,
//! then the data root and `<P>`, each directory only if the record names it
//! and it is empty. The `Watch` is told of each step, so that the tests point
//! the setup at a temporary root and inject a failure at each one.

use std::collections::BTreeSet;
use std::io::Read as _;
use std::path::PathBuf;
use std::time::Duration;

use cg_agent::custody::{self, Handle, Kind, Policy, Principal, Shape};
use cg_agent::service::layout::{classify_names, HostState, Names};
use cg_agent::service::{InstalledPaths, READ_LIMIT};
use windows_sys::Win32::Foundation::GENERIC_READ;
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL,
};

use crate::codes::{Refusal, OWN_STATE, UNTRUSTED_PATH};
use crate::record::{Created, Record, RECORD};

const SHARE_ALL: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
/// A held directory shares no deletion: no one can open it to rename or
/// delete it while it is held. Windows records a handle's sharing only when
/// it holds a data access, so the hold lists the directory too; with
/// attributes and security alone, the directory could still be renamed.
const HOLD_SHARE: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE;
const HOLD_ACCESS: u32 = FILE_LIST_DIRECTORY | READ_CONTROL | FILE_READ_ATTRIBUTES;

/// Who and what the setup checks and creates. The tests put the current
/// user where these name SYSTEM and Administrators.
#[derive(Debug, Clone)]
pub struct SetupCustody {
    /// The service's check of `<P>` and the data root (the Amendment
    /// 2026-11-04, item 3).
    pub directories: Policy,
    /// The record's and the staged files' check, as item 6.4 of the
    /// Amendment 2026-11-09 opens a staged file: owner SYSTEM or
    /// Administrators, a protected DACL for SYSTEM and Administrators alone.
    pub staged: Policy,
    /// `<P>` and the data root as the setup creates them: owner
    /// Administrators and a protected DACL for SYSTEM and Administrators.
    pub directory_sddl: String,
    /// The record and each staged file as the setup creates them.
    pub file_sddl: String,
}

impl SetupCustody {
    /// The machine's.
    pub fn system() -> Self {
        let both = vec![Principal::System, Principal::Administrators];
        SetupCustody {
            directories: Policy::shared(),
            staged: Policy {
                owners: both.clone(),
                allowed: both,
                protected: true,
            },
            directory_sddl: "O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)".to_string(),
            file_sddl: "O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)".to_string(),
        }
    }
}

/// A name the setup holds, creates or removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Name {
    ProductRoot,
    DataRoot,
    StagedAnchor,
    StagedConfig,
    Record,
}

impl Name {
    pub fn path(self, paths: &InstalledPaths) -> PathBuf {
        match self {
            Name::ProductRoot => paths.product_root.clone(),
            Name::DataRoot => paths.data_root.clone(),
            Name::StagedAnchor => paths.staged_anchor(),
            Name::StagedConfig => paths.staged_config(),
            Name::Record => paths.data_root.join(RECORD),
        }
    }
}

impl From<Created> for Name {
    fn from(created: Created) -> Self {
        match created {
            Created::ProductRoot => Name::ProductRoot,
            Created::DataRoot => Name::DataRoot,
            Created::StagedAnchor => Name::StagedAnchor,
            Created::StagedConfig => Name::StagedConfig,
        }
    }
}

/// Where the setup stands, for `Watch`: each step comes immediately before
/// the operation it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Opening the directory of that name to hold it.
    Hold(Name),
    /// Creating the directory of that name.
    Create(Name),
    /// Removing what an earlier attempt left under that name.
    RemoveEarlier(Name),
    /// Creating the record, or the staged file of that name.
    Write(Name),
    /// The reversal's reading of the record.
    ReadRecord,
    /// Removing what a record names, that name.
    Remove(Name),
}

/// Told of each step. Production watches nothing; the tests make a step fail
/// with a reason, as if Windows had refused it.
pub trait Watch: Sync {
    fn at(&self, step: Step) -> Result<(), String>;
}

impl<F: Fn(Step) -> Result<(), String> + Sync> Watch for F {
    fn at(&self, step: Step) -> Result<(), String> {
        self(step)
    }
}

/// The watch that injects nothing.
pub struct Unwatched;

impl Watch for Unwatched {
    fn at(&self, _: Step) -> Result<(), String> {
        Ok(())
    }
}

static UNWATCHED: Unwatched = Unwatched;

/// What the staging and the reversal are told.
pub struct Setup<'a> {
    pub paths: InstalledPaths,
    pub custody: SetupCustody,
    /// How long an operation may wait on another process.
    pub wait: Duration,
    pub watch: &'a (dyn Watch + 'a),
}

impl Setup<'static> {
    /// The machine's, at the fixed paths.
    pub fn system(paths: InstalledPaths) -> Self {
        Setup {
            paths,
            custody: SetupCustody::system(),
            wait: custody::OPEN_WAIT,
            watch: &UNWATCHED,
        }
    }
}

/// Why a step did not complete: an untrusted name (27), or a failure of the
/// setup's own state (40).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Fault {
    Untrusted(String),
    Failed(String),
}

impl Fault {
    fn reason(&self) -> &str {
        match self {
            Fault::Untrusted(reason) | Fault::Failed(reason) => reason,
        }
    }

    fn refusal(&self) -> Refusal {
        match self {
            Fault::Untrusted(reason) => {
                Refusal::new(UNTRUSTED_PATH, format!("an untrusted path: {reason}"))
            }
            Fault::Failed(reason) => own_state(reason),
        }
    }
}

fn own_state(reason: &str) -> Refusal {
    Refusal::new(
        OWN_STATE,
        format!("the setup could not write or check its own state: {reason}"),
    )
}

/// Hold the directory `name`, as found or, when `create` allows it, created
/// new; then check it through the held handle. Absent where it may not be
/// created, a name that is there by the creation, or what fails the check is
/// untrusted.
fn hold(setup: &Setup<'_>, name: Name, create: bool) -> Result<(Handle, bool), Fault> {
    let path = name.path(&setup.paths);
    let open = || {
        custody::open_existing(&path, HOLD_ACCESS, HOLD_SHARE, setup.wait).map_err(Fault::Untrusted)
    };
    setup.watch.at(Step::Hold(name)).map_err(Fault::Untrusted)?;
    let (handle, created) = match open()? {
        Some(handle) => (handle, false),
        None if create => {
            setup.watch.at(Step::Create(name)).map_err(Fault::Failed)?;
            if let Err(reason) = custody::create_new_directory(&path, &setup.custody.directory_sddl)
            {
                return Err(match custody::shape(&path, setup.wait) {
                    Ok(None) => Fault::Failed(reason),
                    _ => Fault::Untrusted(format!(
                        "{} was there when the setup created it",
                        path.display()
                    )),
                });
            }
            let handle = open()?
                .ok_or_else(|| Fault::Untrusted(format!("{} went once created", path.display())))?;
            (handle, true)
        }
        None => {
            return Err(Fault::Untrusted(format!(
                "{} does not exist",
                path.display()
            )))
        }
    };
    custody::check_shape(&handle, &path, Kind::Directory).map_err(Fault::Untrusted)?;
    custody::check_security(&handle, &path, &setup.custody.directories)
        .map_err(Fault::Untrusted)?;
    Ok((handle, created))
}

/// Delete what is at `name` without following it; nothing there is done. A
/// directory there is untrusted and left.
fn remove_file(setup: &Setup<'_>, name: Name, step: Step) -> Result<(), Fault> {
    let path = name.path(&setup.paths);
    setup.watch.at(step).map_err(Fault::Failed)?;
    let Some(handle) =
        custody::open_existing(&path, DELETE | FILE_READ_ATTRIBUTES, SHARE_ALL, setup.wait)
            .map_err(Fault::Failed)?
    else {
        return Ok(());
    };
    let info = custody::file_information(&handle, &path).map_err(Fault::Failed)?;
    if info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
        return Err(Fault::Untrusted(format!(
            "{} is a directory",
            path.display()
        )));
    }
    custody::delete_through(handle, &path, setup.wait).map_err(Fault::Failed)
}

/// Release the held directory `name` and delete it if it is empty.
fn remove_directory(setup: &Setup<'_>, name: Name, held: Handle) -> Result<(), Fault> {
    drop(held);
    let path = name.path(&setup.paths);
    setup.watch.at(Step::Remove(name)).map_err(Fault::Failed)?;
    let Some(handle) =
        custody::open_existing(&path, DELETE | FILE_READ_ATTRIBUTES, SHARE_ALL, setup.wait)
            .map_err(Fault::Failed)?
    else {
        return Ok(());
    };
    custody::check_shape(&handle, &path, Kind::Directory).map_err(Fault::Failed)?;
    if !custody::entries(&path, setup.wait)
        .map_err(Fault::Failed)?
        .is_empty()
    {
        return Ok(());
    }
    custody::delete_through(handle, &path, setup.wait).map_err(Fault::Failed)
}

/// Remove what `record` names, in item 5's order: the staged files, the
/// record, then the data root and `<P>`, each only if named and empty. The
/// first failure stops it, so that the record stays while a staged file does.
fn remove_named(
    setup: &Setup<'_>,
    record: &Record,
    product_root: Handle,
    data_root: Handle,
) -> Result<(), Fault> {
    for name in [Created::StagedConfig, Created::StagedAnchor] {
        if record.created.contains(&name) {
            remove_file(setup, name.into(), Step::Remove(name.into()))?;
        }
    }
    remove_file(setup, Name::Record, Step::Remove(Name::Record))?;
    for (created, held) in [
        (Created::DataRoot, data_root),
        (Created::ProductRoot, product_root),
    ] {
        if record.created.contains(&created) {
            remove_directory(setup, created.into(), held)?;
        }
    }
    Ok(())
}

/// What a route that stages writes.
#[derive(Clone, Copy)]
pub struct Files<'f> {
    pub anchor: &'f [u8],
    pub config: &'f str,
    pub fingerprints: &'f BTreeSet<String>,
}

/// An attempt whose record is written: `<P>` and the data root stay held
/// until it is dropped or removed.
pub struct Attempt {
    product_root: Handle,
    data_root: Handle,
    record: Record,
}

impl std::fmt::Debug for Attempt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Attempt")
            .field("record", &self.record)
            .finish_non_exhaustive()
    }
}

impl Attempt {
    pub fn record(&self) -> &Record {
        &self.record
    }

    /// A refusal or failure after the record: remove what the record names
    /// (item 5), then return `refusal`, its line naming a removal that
    /// failed. The code stays `refusal`'s.
    pub fn refuse(self, setup: &Setup<'_>, refusal: Refusal) -> Refusal {
        match remove_named(setup, &self.record, self.product_root, self.data_root) {
            Ok(()) => refusal,
            Err(fault) => Refusal {
                code: refusal.code,
                line: format!(
                    "{}; what the setup created could not be removed: {}",
                    refusal.line,
                    fault.reason()
                ),
            },
        }
    }
}

/// Item 5, after items 3 and 4, on every route but a migration's: hold `<P>`
/// and the data root, remove what an earlier attempt left, and write the
/// record, then, on a route that stages (`files`), the anchor and the
/// configuration. Only a route that stages creates `<P>` or the data root.
/// A failure after the record removes what it names before this returns.
pub fn stage(
    setup: &Setup<'_>,
    files: Option<Files<'_>>,
    service_running: bool,
) -> Result<Attempt, Refusal> {
    let staging = files.is_some();
    let (product_root, created_product_root) =
        hold(setup, Name::ProductRoot, staging).map_err(|fault| fault.refusal())?;
    let (data_root, created_data_root) =
        hold(setup, Name::DataRoot, staging).map_err(|fault| fault.refusal())?;
    for name in [Name::StagedConfig, Name::StagedAnchor, Name::Record] {
        remove_file(setup, name, Step::RemoveEarlier(name)).map_err(|fault| fault.refusal())?;
    }

    let mut created = BTreeSet::new();
    if created_product_root {
        created.insert(Created::ProductRoot);
    }
    if created_data_root {
        created.insert(Created::DataRoot);
    }
    if staging {
        created.extend([Created::StagedAnchor, Created::StagedConfig]);
    }
    let attempt = Attempt {
        product_root,
        data_root,
        record: Record {
            created,
            fingerprints: files.map(|f| f.fingerprints.clone()).unwrap_or_default(),
            service_running,
        },
    };

    let write = |name: Name, bytes: &[u8]| -> Result<(), String> {
        setup.watch.at(Step::Write(name))?;
        custody::create_new_file(&name.path(&setup.paths), &setup.custody.file_sddl, bytes)
    };
    let written = write(Name::Record, &attempt.record.to_bytes()).and_then(|()| match files {
        Some(files) => write(Name::StagedAnchor, files.anchor)
            .and_then(|()| write(Name::StagedConfig, files.config.as_bytes())),
        None => Ok(()),
    });
    match written {
        Ok(()) => Ok(attempt),
        Err(reason) => Err(attempt.refuse(setup, own_state(&reason))),
    }
}

/// The record, read as item 6.4 of the Amendment 2026-11-09 opens a staged
/// file, through a handle that excludes writers, and checked for its form:
/// the record's checks (item 8). `Ok(None)` when it is absent; the error
/// names the record and why it fails.
pub fn read_record(setup: &Setup<'_>) -> Result<Option<Record>, String> {
    let path = Name::Record.path(&setup.paths);
    let Some(handle) = custody::open_existing(
        &path,
        GENERIC_READ | READ_CONTROL | FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ,
        setup.wait,
    )?
    else {
        return Ok(None);
    };
    custody::check_shape(&handle, &path, Kind::File)?;
    custody::check_security(&handle, &path, &setup.custody.staged)?;
    let info = custody::file_information(&handle, &path)?;
    if (u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow) > READ_LIMIT {
        return Err(format!(
            "{} is larger than {READ_LIMIT} bytes",
            path.display()
        ));
    }
    let mut bytes = Vec::new();
    handle
        .into_file()
        .take(READ_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    Record::parse(&bytes)
        .map(Some)
        .map_err(|reason| format!("{} is refused: {reason}", path.display()))
}

/// What the reversal did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reversal {
    /// Nothing changed, and why.
    Unchanged(String),
    /// What the record named is removed; the product service is to be
    /// started with no argument when `start` is true.
    Removed { start: bool },
}

/// Item 7, `cg-setup reverse`: read the host's state as preflight does,
/// then, where preflight would have staged, remove what the record names.
/// Exit 0 is `Ok`; a host whose names cannot be read, a record that fails
/// its checks and a removal that fails are 40.
pub fn reverse(setup: &Setup<'_>) -> Result<Reversal, Refusal> {
    let paths = &setup.paths;
    let unreadable =
        |reason: &str| own_state(&format!("the host's names cannot be read: {reason}"));
    let names = Names::read(paths, setup.wait).map_err(|reason| unreadable(&reason))?;
    // The data root's listing, in the order of item 3's states.
    if names.reparse_point(paths).is_none() && !names.migration_in_progress() {
        if let Some(reason) = names.unlisted() {
            return Err(unreadable(reason));
        }
    }
    let unchanged = |why: &str| Ok(Reversal::Unchanged(why.to_string()));
    match classify_names(&names, paths, &setup.custody.directories, setup.wait) {
        HostState::Untrusted(reason) => return unchanged(&format!("an untrusted path: {reason}")),
        HostState::MigrationInProgress => return unchanged("a migration is in progress"),
        HostState::ConsoleLayout => return unchanged("a console layout"),
        HostState::Enrolled | HostState::Pending | HostState::Fresh => {}
    }
    if names.data_root() != Some(Shape::Directory) {
        return unchanged("there is no data root");
    }
    let product_root = match hold(setup, Name::ProductRoot, false) {
        Ok((handle, _)) => handle,
        Err(fault) => return unchanged(fault.reason()),
    };
    let data_root = match hold(setup, Name::DataRoot, false) {
        Ok((handle, _)) => handle,
        Err(fault) => return unchanged(fault.reason()),
    };

    let read = setup
        .watch
        .at(Step::ReadRecord)
        .and_then(|()| read_record(setup));
    match read {
        Ok(None) => unchanged("there is no record"),
        Ok(Some(record)) => {
            remove_named(setup, &record, product_root, data_root)
                .map_err(|fault| own_state(fault.reason()))?;
            Ok(Reversal::Removed {
                start: record.service_running,
            })
        }
        Err(reason) => {
            let removed = [Name::StagedConfig, Name::StagedAnchor, Name::Record]
                .into_iter()
                .try_for_each(|name| remove_file(setup, name, Step::Remove(name)));
            let line = match removed {
                Ok(()) => reason,
                Err(fault) => format!(
                    "{reason}; the staged files and the record could not be removed: {}",
                    fault.reason()
                ),
            };
            Err(own_state(&line))
        }
    }
}
