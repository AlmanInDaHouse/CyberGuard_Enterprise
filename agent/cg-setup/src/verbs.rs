//! `cg-setup`'s verbs (SPEC-024 Amendment 2026-11-10, item 9), after the
//! platform and the elevation, which `src/main.rs` checks first: each one in
//! the order its item gives, on a `Machine`, which names the paths, the
//! product service and the installed binary. The machine's are the fixed
//! paths under the folders the OS reports and `CyberGuardAgent`; the tests
//! point a machine at temporary roots and at test services of their own, and
//! give it a wizard of their own.

use std::path::PathBuf;
use std::time::Instant;

use cg_agent::custody;
use cg_agent::service::InstalledPaths;

use crate::codes::{Refusal, OPERATIONAL, OWN_STATE};
use crate::observer::{self, LogPosition};
use crate::preflight::{self, decide_again, parse_migration, take_fields, Host, Plan, Request};
use crate::scm::{self, Manager, Product, Route};
use crate::service::{migration_arguments, start_arguments, PRODUCT_SERVICE};
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
    /// Amendment 2026-11-13, point 1); a machine without one takes that
    /// window as none.
    pub wizard: Option<&'a dyn Wizard>,
}

impl Machine<'static> {
    /// The machine's: the fixed paths under the `%ProgramData%` and
    /// `%ProgramFiles%` the OS reports, and the product service. A folder
    /// Windows does not report is 40 (item 2).
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
            wizard: None,
        })
    }
}

/// The installed binary under `program_files`.
pub fn installed_binary(program_files: &std::path::Path) -> PathBuf {
    program_files
        .join("CyberGuard")
        .join("Agent")
        .join("cg-agent.exe")
}

impl Machine<'_> {
    fn paths(&self) -> &InstalledPaths {
        &self.host.paths
    }

    /// The agent's log directory, under the data root configure holds.
    pub fn log_dir(&self) -> PathBuf {
        self.paths().data_root.join("logs")
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
    // PROBE: no reading of the service before the dialog.
    let product = match plan {
        Plan::Ask(_) => {
            version::downgrade(&machine.binary, own_numeric())?;
            Product::Absent
        }
        _ => check(machine, &scm, route)?,
    };
    let (plan, product) = match (plan, machine.wizard) {
        // PROBE: the first reading kept after the dialog.
        (Plan::Ask(offer), Some(wizard)) => (ask(machine, wizard, &scm, offer)?.0, product),
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
    // PROBE: no second reading of the service.
    let _ = scm;
    version::downgrade(&machine.binary, own_numeric())?;
    let product = Product::Absent;
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
