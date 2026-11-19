//! `cg-setup`'s verbs (SPEC-024 Amendment 2026-11-10, item 9), after the
//! platform and the elevation, which `src/main.rs` checks first: each one in
//! the order its item gives, on a `Machine`, which names the paths, the
//! product service and the installed binary. The machine's are the fixed
//! paths under the folders the OS reports and `CyberGuardAgent`; the tests
//! point a machine at temporary roots and at test services of their own, and
//! give it a wizard of their own.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use cg_agent::custody;
use cg_agent::service::layout::{classify, HostState};
use cg_agent::service::names;
use cg_agent::service::InstalledPaths;
use windows_sys::Win32::Foundation::ERROR_SERVICE_SPECIFIC_ERROR;

use crate::codes::{
    Refusal, CONSOLE_LAYOUT, OPERATIONAL, OWN_STATE, PURGE_INCOMPLETE, PURGE_REFUSED,
    UNTRUSTED_PATH,
};
use crate::dialog::SystemWizard;
use crate::observer::{self, LogPosition};
use crate::preflight::{self, decide_again, parse_migration, take_fields, Host, Plan, Request};
use crate::scm::{self, Ended, Manager, Product, Route};
use crate::service::{
    migration_arguments, start_arguments, PRODUCT_SERVICE, PURGE, PURGE_BEFORE_REMOVAL,
};
use crate::staging::{self, Files, Setup};
use crate::version::{self, numeric, SETUP_VERSION};
use crate::wizard::{Again, Answer, Choice, Closed, Offer, Wizard};

/// Where the verbs act.
pub struct Machine<'a> {
    /// What preflight reads, and where.
    pub host: Host,
    /// What the staging, configure's hold and the reversal are told; its
    /// paths are `host`'s.
    pub setup: Setup<'a>,
    /// The product service's name.
    pub service: String,
    /// The installed binary, `<ProgramFiles>\CyberGuard\Agent\cg-agent.exe`.
    pub binary: PathBuf,
    /// The wizard, which shows only with the setup's full window (the
    /// Amendment 2026-11-13, point 1); a machine without one, as the tests
    /// may have it, takes that window as none.
    pub wizard: Option<&'a dyn Wizard>,
}

impl Machine<'static> {
    /// The machine's: the fixed paths under the `%ProgramData%` and
    /// `%ProgramFiles%` the OS reports, the product service, and the setup's
    /// wizard. A folder Windows does not report is 40 (item 2).
    pub fn system() -> Result<Self, Refusal> {
        let host = Host::system()?;
        let program_files = custody::program_files().map_err(|reason| {
            Refusal::new(OWN_STATE, format!("the host cannot be read: {reason}"))
        })?;
        Ok(Machine {
            setup: Setup::system(host.paths.clone()),
            host,
            service: PRODUCT_SERVICE.to_string(),
            binary: installed_binary(&program_files),
            wizard: Some(&SystemWizard),
        })
    }
}

/// The installed binary under `program_files`.
pub fn installed_binary(program_files: &std::path::Path) -> PathBuf {
    program_files
        .join(names::PRODUCT_ROOT)
        .join(names::SERVICE_DIRECTORY)
        .join(names::EXECUTABLE)
}

impl Machine<'_> {
    fn paths(&self) -> &InstalledPaths {
        &self.host.paths
    }

    /// The agent's log directory, under the data root configure holds.
    pub fn log_dir(&self) -> PathBuf {
        self.paths().data_root.join(names::LOGS_DIRECTORY)
    }
}

/// `X.Y.Z` of the version the setup was built with.
fn own_numeric() -> [u16; 3] {
    // `version.rs` of cg-agent refuses any other build-time input.
    numeric(SETUP_VERSION).expect("the setup's own version is X.Y.Z or X.Y.Z-dev")
}

/// `preflight` (the Amendment 2026-11-09, item 3, with the Amendment
/// 2026-11-10, items 2 to 6, and the Amendment 2026-11-11, point 1): the
/// state, the request and, on a route that stages, its inputs; the
/// conflicting service; the downgrade; on the wizard's route, the wizard and
/// the second reading (the Amendment 2026-11-13, points 2 and 3); on every
/// route but a migration's, the staging and its record; the stop; then,
/// after it, the record's rewrite and a refusal's removal.
pub fn preflight(machine: &Machine<'_>, request: &Request) -> Result<(), Refusal> {
    let request = match machine.wizard {
        Some(_) => request.clone(),
        None => request.clone().with_ui(None),
    };
    let plan = preflight::assess(&machine.host, &request)?;
    let route = match plan {
        Plan::Migrate(_) => Route::Migration,
        Plan::Start | Plan::Stage(_) | Plan::Ask(_) => Route::Other,
    };
    let scm = Manager::open()?;
    let product = check(machine, &scm, route)?;
    let (plan, product) = match (plan, machine.wizard) {
        (Plan::Ask(offer), Some(wizard)) => ask(machine, wizard, &scm, offer)?,
        (Plan::Ask(_), None) => return Err(inconsistent("a wizard to ask on a machine with none")),
        (plan, _) => (plan, product),
    };
    let attempt = match &plan {
        // It creates and changes nothing under <P>, and writes no record.
        Plan::Migrate(_) => return scm::stop(&scm, &machine.service).result,
        Plan::Start => staging::stage(&machine.setup, None, product.running())?,
        Plan::Stage(staged) => staging::stage(
            &machine.setup,
            Some(Files {
                anchor: &staged.anchor,
                config: &staged.config,
                fingerprints: &staged.fingerprints,
            }),
            product.running(),
        )?,
        Plan::Ask(_) => return Err(inconsistent("the wizard's answer, which stages or starts")),
    };
    let stop = scm::stop(&scm, &machine.service);
    attempt
        .after_stop(&machine.setup, stop.found_running, stop.result)
        .map(drop)
}

/// Code 40 for what the wizard's route cannot reach: the elevated process
/// exits with one of item 7's codes rather than a panic's.
fn inconsistent(what: &str) -> Refusal {
    Refusal::new(
        OWN_STATE,
        format!("the setup's own state is inconsistent: {what}"),
    )
}

/// The conflicting service (26), then the downgrade (29).
fn check(machine: &Machine<'_>, scm: &Manager, route: Route) -> Result<Product, Refusal> {
    let product = scm::check_product_service(
        scm,
        &machine.service,
        &machine.binary,
        &machine.paths().config,
        route,
    )?;
    version::downgrade(&machine.binary, own_numeric())?;
    Ok(product)
}

/// The wizard's route (the Amendment 2026-11-13, points 3 to 5): the process
/// that started the setup held (40), the dialog until an accepted submission
/// or a choice to keep (22 when it closes otherwise), that process still
/// running (22), then the state, the conflicting service and the downgrade
/// read again; the plan and the service the record reads are this second
/// reading's.
fn ask(
    machine: &Machine<'_>,
    wizard: &dyn Wizard,
    scm: &Manager,
    offer: Offer,
) -> Result<(Plan, Product), Refusal> {
    let starter = wizard.hold_starter()?;
    let mut take = |fields: &_| take_fields(&machine.host, fields);
    let (choice, staged) = match wizard.ask(offer, starter.as_ref(), &mut take)? {
        Answer::Input(staged) => (Choice::Input, Some(staged)),
        Answer::Keep if offer == Offer::Pending => (Choice::Keep, None),
        Answer::Keep => return Err(inconsistent("a choice to keep on a fresh host")),
        Answer::Closed(closed) => return Err(closed.refusal()),
    };
    if !starter.running() {
        return Err(Closed::StarterExited.refusal());
    }
    let state = cg_agent::service::layout::classify(
        &machine.host.paths,
        &machine.host.directories,
        machine.host.wait,
    );
    let again = decide_again(&state, choice)?;
    let product = check(machine, scm, Route::Other)?;
    let plan = match (again, staged) {
        (Again::Stage, Some(staged)) => Plan::Stage(staged),
        (Again::Start, None) => Plan::Start,
        _ => return Err(inconsistent("the second reading of the wizard's answer")),
    };
    Ok((plan, product))
}

/// `configure` (the Amendment 2026-11-09, items 5, 7 and 8, with the
/// Amendment 2026-11-10, item 8, and the Amendment 2026-11-11, point 4): on
/// a route other than a migration's, `<P>` and the data root held and the
/// record and the staged files checked; the installed binary's version; the
/// failure actions; the log's position; the start; the record's deletion;
/// then the outcome, observed for 60 seconds from the start request. A
/// deletion of the record that fails keeps the outcome's code, its line
/// naming the record left: with 0, `Err` carries code 0 and that line (the
/// Amendment 2026-11-11 (second), point 4).
pub fn configure(machine: &Machine<'_>, request: &Request) -> Result<(), Refusal> {
    // Configure takes the migration's variables from its own command line,
    // which preflight checked in the same form (item 8).
    let migration = request
        .migration_requested()
        .then(|| {
            parse_migration(
                request.migrate_legacy.as_deref(),
                request.trust_anchors.as_deref(),
            )
        })
        .transpose()
        .map_err(|reason| {
            Refusal::new(
                OWN_STATE,
                format!("configure's migration variables are refused: {reason}"),
            )
        })?;
    let held = match migration {
        Some(_) => None,
        None => Some(staging::configure(&machine.setup)?),
    };
    version::configure_version(&machine.binary, own_numeric(), SETUP_VERSION)?;
    let scm = Manager::open()?;
    scm::set_failure_actions(&scm, &machine.service)?;
    let log_dir = machine.log_dir();
    let position = LogPosition::record(&log_dir)?;
    let arguments = match (&migration, &held) {
        (Some(migration), _) => migration_arguments(migration),
        (None, Some(held)) => start_arguments(&held.record().fingerprints),
        (None, None) => {
            unreachable!("configure holds the record on a route other than a migration's")
        }
    };
    let started = Instant::now();
    scm::start(&scm, &machine.service, &arguments)?;
    let deleted = held
        .as_ref()
        .map_or(Ok(()), |held| held.delete_record(&machine.setup));
    let observed = observer::observe(&scm, &machine.service, &log_dir, position, started);
    match (observed, deleted) {
        (Ok(()), Ok(())) => Ok(()),
        // Operational all the same: 0, with the line naming the record left
        // (the Amendment 2026-11-11 (second), point 4).
        (Ok(()), Err(failed)) => Err(Refusal {
            code: OPERATIONAL,
            line: failed.line,
        }),
        (Err(refusal), Ok(())) => Err(refusal),
        // The outcome's code, its line naming the record left too.
        (Err(refusal), Err(failed)) => Err(Refusal {
            code: refusal.code,
            line: format!(
                "{}; and {}",
                refusal.line,
                failed
                    .line
                    .strip_prefix("cg-setup: ")
                    .unwrap_or(&failed.line)
            ),
        }),
    }
}

/// `reverse` (the Amendment 2026-11-10, item 7, with the Amendment
/// 2026-11-11, point 2): the removal the record names, then, when the record
/// found the service running, its start with no argument.
pub fn reverse(machine: &Machine<'_>) -> Result<(), Refusal> {
    staging::reverse(&machine.setup)?.finish(|| {
        let scm = Manager::open()?;
        scm::start_if_stopped(&scm, &machine.service).map(drop)
    })
}

/// `purge` with `--confirm=yes` (SPEC-024 Amendment 2026-11-15, point 3),
/// `within` being its 300 seconds: `<P>` absent is 0; the host's state as
/// preflight reads it (27, 28); the product service as preflight checks it
/// (26, and 46 when it is absent); its stop (31); then its start with
/// `--purge`, whose end `outcome` reads: 0, 46 when the purge removed
/// nothing, the service then started again if it was running, 47 otherwise,
/// the service left as it is. It holds no handle under `<P>`.
pub fn purge(machine: &Machine<'_>, within: Duration) -> Result<(), Refusal> {
    let paths = machine.paths();
    if let Ok(None) = custody::shape(&paths.product_root, machine.host.wait) {
        return Ok(());
    }
    match classify(paths, &machine.host.directories, machine.host.wait) {
        HostState::Untrusted(reason) => {
            return Err(Refusal::new(
                UNTRUSTED_PATH,
                format!("the purge refuses an untrusted path: {reason}"),
            ))
        }
        HostState::MigrationInProgress => {
            return Err(Refusal::new(
                CONSOLE_LAYOUT,
                "the purge refuses a migration in progress: finish or reverse it first",
            ))
        }
        HostState::ConsoleLayout => {
            return Err(Refusal::new(
                CONSOLE_LAYOUT,
                "the purge refuses a console layout: the data directory holds a console installation's identity files",
            ))
        }
        HostState::Enrolled | HostState::Pending | HostState::Fresh => {}
    }
    let scm = Manager::open()?;
    let name = &machine.service;
    let product =
        scm::check_product_service(&scm, name, &machine.binary, &paths.config, Route::Other)?;
    if product == Product::Absent {
        return Err(Refusal::new(
            PURGE_REFUSED,
            format!("the purge removed nothing: the product service {name} is absent, and no other process may remove what only SYSTEM can reach"),
        ));
    }
    let stop = scm::stop(&scm, name);
    stop.result?;
    let removed_nothing = |cause: String| {
        let again = if stop.found_running {
            restart_note(scm::start_if_stopped(&scm, name))
        } else {
            String::new()
        };
        Refusal::new(
            PURGE_REFUSED,
            format!("the purge removed nothing: {cause}{again}"),
        )
    };
    // `--purge` reaches the service's start only through `ServiceMain`, so a
    // start the SCM refuses, a process that died before its dispatcher
    // connected included, removed nothing (46).
    if let Err(refusal) = scm::start(&scm, name, &[PURGE.to_string()]) {
        return Err(removed_nothing(text_of(&refusal).to_string()));
    }
    match outcome(&scm::wait_stopped(&scm, name, within), within) {
        Outcome::Purged => Ok(()),
        Outcome::RemovedNothing(cause) => Err(removed_nothing(cause)),
        Outcome::Incomplete(cause) => Err(Refusal::new(
            PURGE_INCOMPLETE,
            format!("the purge may have removed some of the product's data: {cause}; the service is left as it is, and the purge run again names what remains"),
        )),
    }
}

/// What the service's purge start ended with, for the purge's code (point
/// 3.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// 0.
    Purged,
    /// 46, with its cause.
    RemovedNothing(String),
    /// 47, with its cause.
    Incomplete(String),
}

/// Point 3.5's reading of the end `wait_stopped` read: `STOPPED` with
/// `dwWin32ExitCode` 0 is the code 0; with 1066 the code is
/// `dwServiceSpecificExitCode`; any other value, 1067 for a process that died
/// included, is no code of the service's. 0 is 0; a code the purge start gives
/// before any removal is 46; any other code, no code, a state that cannot be
/// read and any other state at the end are 47.
pub fn outcome(ended: &Ended, within: Duration) -> Outcome {
    let code = match ended {
        Ended::Stopped { win32: 0, .. } => Some(0),
        Ended::Stopped {
            win32: ERROR_SERVICE_SPECIFIC_ERROR,
            specific,
        } => Some(*specific),
        _ => None,
    };
    match (ended, code) {
        (_, Some(0)) => Outcome::Purged,
        (_, Some(code)) if PURGE_BEFORE_REMOVAL.contains(&code) => Outcome::RemovedNothing(
            format!("the service's purge start stopped with code {code}, which it gives before any removal"),
        ),
        (_, Some(15)) => Outcome::Incomplete(
            "the service's purge start stopped with code 15; when it left a name or ran out of its 120 seconds, its entry 1002 in the Application log names what it left".to_string(),
        ),
        (_, Some(code)) => {
            Outcome::Incomplete(format!("the service's purge start stopped with code {code}"))
        }
        (Ended::Stopped { win32, .. }, None) => Outcome::Incomplete(format!(
            "the service's purge start ended with Win32 error {win32}"
        )),
        (Ended::Unreadable(reason), _) => Outcome::Incomplete(reason.clone()),
        (Ended::Still(state), _) => Outcome::Incomplete(format!(
            "the service's purge start did not report STOPPED within {} seconds of its start; its state then was {}",
            within.as_secs(),
            state_name(*state)
        )),
    }
}

/// A service state's name, as the SCM's constants name it.
fn state_name(state: u32) -> String {
    match state {
        1 => "STOPPED".to_string(),
        2 => "START_PENDING".to_string(),
        3 => "STOP_PENDING".to_string(),
        4 => "RUNNING".to_string(),
        5 => "CONTINUE_PENDING".to_string(),
        6 => "PAUSE_PENDING".to_string(),
        7 => "PAUSED".to_string(),
        other => format!("{other}"),
    }
}

/// What 46's line adds after a service that was running: whether
/// `scm::start_if_stopped` started it again, or why it could not.
pub fn restart_note(started: Result<bool, Refusal>) -> String {
    match started {
        Ok(true) => "; the service was started again".to_string(),
        Ok(false) => "; the service was not stopped, so it was not started again".to_string(),
        Err(refusal) => {
            // `start_if_stopped` words its failure as the reversal's.
            let text = text_of(&refusal);
            let cause = text
                .strip_prefix("the reversal could not start the service ")
                .and_then(|rest| rest.split_once(": ").map(|(_, cause)| cause))
                .unwrap_or(text);
            format!("; the service could not be started again: {cause}")
        }
    }
}

/// A refusal's text without its `cg-setup: ` prefix.
fn text_of(refusal: &Refusal) -> &str {
    refusal
        .line
        .strip_prefix("cg-setup: ")
        .unwrap_or(&refusal.line)
}
