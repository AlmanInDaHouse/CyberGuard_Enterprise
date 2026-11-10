//! Preflight (SPEC-024 Amendment 2026-11-09, item 3), up to what it stages:
//! the state of the host, the first that applies, read by
//! `cg_agent::service::layout::classify` with the service's check of `<P>` and
//! the data root; the route that state and the request allow; and, on a route
//! that stages, the inputs, the trust anchor against its fingerprints and the
//! generated configuration. Nothing here changes the host. The conflicting
//! service, the downgrade, the staging, the record and the stop come after it.

use std::collections::BTreeSet;
use std::io::Read as _;
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use cg_agent::custody::{self, Policy, Principal};
use cg_agent::service::anchor::{check_certificates, is_fingerprint};
use cg_agent::service::layout::{classify, HostState};
use cg_agent::service::migration::{self, Choice, DEFAULT_REFUSAL};
use cg_agent::service::{InstalledPaths, READ_LIMIT};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_READ, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
};
use zeroize::Zeroizing;

use crate::codes::{
    Refusal, ALREADY_CONFIGURED, ANCHOR, BOOTSTRAP_ACCESS, CONSOLE_LAYOUT, INVALID_INPUT, NO_INPUT,
    OWN_STATE, UNTRUSTED_PATH,
};
use crate::inputs::parse_bootstrap;
use crate::staged::{check_staged, placeholder_id, staged_anchor, staged_config};

/// What the setup is asked, from its bundle variables (item 2). An empty
/// value is a variable not given: Burn expands one that is unset to nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Request {
    /// `CG_BOOTSTRAP`.
    pub bootstrap: Option<PathBuf>,
    /// `CG_MIGRATE_LEGACY`.
    pub migrate_legacy: Option<String>,
    /// `CG_TRUST_ANCHORS`.
    pub trust_anchors: Option<String>,
}

fn given(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

impl Request {
    /// The request of these variables, an empty one not given.
    pub fn new(
        bootstrap: Option<&str>,
        migrate_legacy: Option<&str>,
        trust_anchors: Option<&str>,
    ) -> Self {
        Request {
            bootstrap: given(bootstrap).map(PathBuf::from),
            migrate_legacy: given(migrate_legacy).map(str::to_string),
            trust_anchors: given(trust_anchors).map(str::to_string),
        }
    }

    /// Either migration variable is given.
    pub fn migration_requested(&self) -> bool {
        self.migrate_legacy.is_some() || self.trust_anchors.is_some()
    }
}

/// `CG_MIGRATE_LEGACY` and `CG_TRUST_ANCHORS` in the form the service takes
/// them (the Amendment 2026-11-05, item 1): `remove` or `keep`, and one or
/// more comma-separated fingerprints. The error is the reason.
pub fn parse_migration(
    migrate_legacy: Option<&str>,
    trust_anchors: Option<&str>,
) -> Result<migration::Request, String> {
    let choice = match given(migrate_legacy) {
        Some("remove") => Choice::Remove,
        Some("keep") => Choice::Keep,
        Some(_) => return Err("CG_MIGRATE_LEGACY is not remove or keep".to_string()),
        None => return Err("CG_TRUST_ANCHORS is given without CG_MIGRATE_LEGACY".to_string()),
    };
    let Some(list) = given(trust_anchors) else {
        return Err("CG_MIGRATE_LEGACY is given without CG_TRUST_ANCHORS".to_string());
    };
    let mut fingerprints = BTreeSet::new();
    for value in list.split(',') {
        if !is_fingerprint(value) {
            return Err(
                "CG_TRUST_ANCHORS holds a value that is not sha256: and 64 lowercase \
                        hexadecimal digits, the values separated by commas alone"
                    .to_string(),
            );
        }
        fingerprints.insert(value.to_string());
    }
    Ok(migration::Request {
        choice,
        fingerprints,
    })
}

/// What a state and a request allow, before any input is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// The migration route: the service checks what it migrates.
    Migrate(migration::Request),
    /// The service is started with no argument; no token is taken.
    Start,
    /// New input, from this bootstrap file, is staged.
    Stage(PathBuf),
}

/// Item 3's table: the state, the first that applies, and the request.
pub fn decide(state: &HostState, request: &Request) -> Result<Decision, Refusal> {
    let migration = |what: &str| -> Result<Decision, Refusal> {
        if request.bootstrap.is_some() || !request.migration_requested() {
            return Err(Refusal::new(
                CONSOLE_LAYOUT,
                format!(
                    "{what}: only the migration route continues, with CG_MIGRATE_LEGACY and \
                     CG_TRUST_ANCHORS and no bootstrap"
                ),
            ));
        }
        parse_migration(
            request.migrate_legacy.as_deref(),
            request.trust_anchors.as_deref(),
        )
        .map(Decision::Migrate)
        .map_err(|reason| {
            Refusal::new(
                INVALID_INPUT,
                format!("invalid migration request: {reason}"),
            )
        })
    };
    match state {
        HostState::Untrusted(reason) => Err(Refusal::new(
            UNTRUSTED_PATH,
            format!("an untrusted path: {reason}"),
        )),
        HostState::MigrationInProgress => migration(
            "a migration is in progress, which the service completes with the arguments it \
             recorded",
        ),
        HostState::ConsoleLayout => migration(DEFAULT_REFUSAL),
        _ if request.migration_requested() => Err(Refusal::new(
            CONSOLE_LAYOUT,
            "a migration is requested, but this host holds no console installation to migrate",
        )),
        HostState::Enrolled => match &request.bootstrap {
            Some(_) => Err(Refusal::new(
                ALREADY_CONFIGURED,
                "the agent is already configured: the data directory holds identity, an \
                 enrolled or preserved agent, and a bootstrap is not taken; run the setup \
                 without one to repair or upgrade it",
            )),
            None => Ok(Decision::Start),
        },
        HostState::Pending => Ok(request
            .bootstrap
            .clone()
            .map_or(Decision::Start, Decision::Stage)),
        HostState::Fresh => match &request.bootstrap {
            Some(path) => Ok(Decision::Stage(path.clone())),
            None => Err(Refusal::new(
                NO_INPUT,
                "no input: a new installation needs a bootstrap file (CG_BOOTSTRAP) or the \
                 wizard's input",
            )),
        },
    }
}

/// What preflight reads, and where. The tests point it at a temporary root,
/// with the current user where the service's names SYSTEM.
#[derive(Debug, Clone)]
pub struct Host {
    pub paths: InstalledPaths,
    /// The service's check of `<P>` and the data root (Amendment 2026-11-04,
    /// item 3).
    pub directories: Policy,
    /// Whom the bootstrap file's owner and allow entries may name (§Data
    /// contracts §3): no unprivileged account, the enrolling user's filtered
    /// token included.
    pub bootstrap: Policy,
    /// The host's name, or `None` for Windows' own, read only by a route
    /// that stages, the one that uses it.
    pub hostname: Option<String>,
    /// How long a read may wait on another process.
    pub wait: Duration,
}

impl Host {
    /// The machine's: the fixed paths under the `%ProgramData%` the OS
    /// reports, the service's directory policy, SYSTEM and Administrators
    /// for the bootstrap file, and Windows' host name. Code 40 if
    /// `%ProgramData%` cannot be read.
    pub fn system() -> Result<Self, Refusal> {
        let program_data = custody::program_data().map_err(own_state)?;
        Ok(Host {
            paths: InstalledPaths::under(&program_data),
            directories: Policy::shared(),
            bootstrap: Policy {
                owners: vec![Principal::System, Principal::Administrators],
                allowed: vec![Principal::System, Principal::Administrators],
                protected: false,
            },
            hostname: None,
            wait: custody::OPEN_WAIT,
        })
    }
}

/// Code 40: what the setup needs of the host to check its own state cannot
/// be read.
fn own_state(reason: String) -> Refusal {
    Refusal::new(OWN_STATE, format!("the host cannot be read: {reason}"))
}

/// What a route that stages takes in, checked: the configuration and the
/// anchor to stage, and the fingerprints the service is started with. Its
/// `Debug` writes neither file.
pub struct Staged {
    pub config: Zeroizing<String>,
    pub anchor: Vec<u8>,
    pub fingerprints: BTreeSet<String>,
}

impl std::fmt::Debug for Staged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Staged")
            .field("config", &"<redacted>")
            .field("anchor_bytes", &self.anchor.len())
            .field("fingerprints", &self.fingerprints)
            .finish()
    }
}

/// Where preflight leaves the request.
#[derive(Debug)]
pub enum Plan {
    Migrate(migration::Request),
    Start,
    Stage(Staged),
}

/// The bootstrap file's bytes: a regular local file whose owner and DACL
/// exclude unprivileged access (§Data contracts §3), or code 23.
fn read_bootstrap(host: &Host, path: &Path) -> Result<Zeroizing<Vec<u8>>, Refusal> {
    let refused = |reason: String| {
        Refusal::new(
            BOOTSTRAP_ACCESS,
            format!("the bootstrap file cannot be used: {reason}"),
        )
    };
    if !path.is_absolute() {
        return Err(refused(format!(
            "{} is not an absolute path",
            path.display()
        )));
    }
    custody::read_local_checked(path, READ_LIMIT, &host.bootstrap, host.wait)
        .map_err(refused)?
        .ok_or_else(|| refused(format!("{} does not exist", path.display())))
}

/// The anchor's source, read once, following what its path names: what it
/// holds is checked by fingerprint, not by where it lies.
fn read_source(path: &Path, wait: Duration) -> Result<Vec<u8>, String> {
    let owned = path.to_path_buf();
    custody::bounded(wait, path, move || {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            // A directory opens, so that it is named; a named pipe's server
            // may identify the setup but not impersonate it.
            .custom_flags(
                FILE_FLAG_BACKUP_SEMANTICS | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
            )
            .open(&owned)
            .map_err(|e| format!("open {}: {e}", owned.display()))?;
        let metadata = file
            .metadata()
            .map_err(|e| format!("inspect {}: {e}", owned.display()))?;
        if !metadata.is_file() {
            return Err(format!("{} is not a regular file", owned.display()));
        }
        let mut bytes = Vec::new();
        file.take(READ_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("read {}: {e}", owned.display()))?;
        if bytes.len() as u64 > READ_LIMIT {
            return Err(format!(
                "{} is larger than {READ_LIMIT} bytes",
                owned.display()
            ));
        }
        Ok(bytes)
    })
}

/// A route that stages: the bootstrap file (23), its inputs (24), the trust
/// anchor against the fingerprints (25), and the configuration generated
/// from them by the service's checks (24).
fn take_in(host: &Host, path: &Path) -> Result<Staged, Refusal> {
    let bytes = read_bootstrap(host, path)?;
    let invalid = |reason: &str| {
        Refusal::new(
            INVALID_INPUT,
            format!("invalid bootstrap file {}: {reason}", path.display()),
        )
    };
    let text = std::str::from_utf8(&bytes).map_err(|_| invalid("the file is not UTF-8"))?;
    let inputs = parse_bootstrap(text).map_err(|reason| invalid(&reason))?;
    if !inputs.anchor_source.is_absolute() {
        return Err(invalid("server.trust_anchor_path must be an absolute path"));
    }

    let anchor_refused = |reason: &str| Refusal::new(ANCHOR, format!("trust anchor: {reason}"));
    let source = read_source(&inputs.anchor_source, host.wait).map_err(|r| anchor_refused(&r))?;
    let name = inputs.anchor_source.display().to_string();
    let certificates = check_certificates(&source, &inputs.fingerprints, &name)
        .map_err(|reason| anchor_refused(&reason))?;

    let hostname = match &host.hostname {
        Some(hostname) => hostname.clone(),
        None => custody::hostname().map_err(own_state)?,
    };
    let config = staged_config(&host.paths, &inputs, &placeholder_id(), &hostname);
    check_staged(&host.paths, &config).map_err(|line| {
        Refusal::new(
            INVALID_INPUT,
            format!("the configuration generated from the input is refused: {line}"),
        )
    })?;
    Ok(Staged {
        config,
        anchor: staged_anchor(&certificates),
        fingerprints: inputs.fingerprints,
    })
}

/// Item 3 after the platform and the elevation: the state, the decision and,
/// on a route that stages, its inputs, read and checked.
pub fn assess(host: &Host, request: &Request) -> Result<Plan, Refusal> {
    let state = classify(&host.paths, &host.directories, host.wait);
    match decide(&state, request)? {
        Decision::Migrate(request) => Ok(Plan::Migrate(request)),
        Decision::Start => Ok(Plan::Start),
        Decision::Stage(path) => take_in(host, &path).map(Plan::Stage),
    }
}

/// Item 3 up to what it stages: the platform, the elevation, then `assess` on
/// the machine's host.
pub fn preflight(request: &Request) -> Result<(Host, Plan), Refusal> {
    crate::platform::check_platform()?;
    crate::platform::check_elevation()?;
    let host = Host::system()?;
    let plan = assess(&host, request)?;
    Ok((host, plan))
}
