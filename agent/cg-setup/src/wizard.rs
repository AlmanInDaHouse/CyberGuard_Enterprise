//! The setup's wizard (SPEC-024 Amendment 2026-11-13), what of it holds on
//! every platform: the signal of the setup's full window (point 1), item 3's
//! table with the states the wizard shows on (point 2) and its second reading
//! after a submission (point 3), the fields and their validation, which is the
//! bootstrap file's (point 4), and what the dialog answers (point 5). The
//! dialog, the process that started the setup (`crate::starter`) and the
//! verb's route (`crate::verbs`) are Windows'.

use zeroize::{Zeroize, Zeroizing};

use crate::codes::{
    Refusal, ALREADY_CONFIGURED, ANCHOR, CONSOLE_LAYOUT, INVALID_INPUT, NO_INPUT, UNTRUSTED_PATH,
};
use crate::inputs::{parse_table, Inputs};
use crate::staged::Staged;

/// `WixBundleUILevel` of the setup's full window, the one value that lets the
/// wizard show (point 1): `2` is `-quiet`, `3` is `-passive`.
pub const FULL_WINDOW: &str = "4";

/// Whether preflight's `--ui` value is the setup's full window. Any other
/// value, or none, is not.
pub fn full_window(ui: Option<&str>) -> bool {
    ui == Some(FULL_WINDOW)
}

/// Item 3's states, the first that applies, as the decision reads them: the
/// service's `HostState` on Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// Why, naming a path and the failed condition.
    Untrusted(String),
    MigrationInProgress,
    ConsoleLayout,
    /// `identity\` is there: an enrolled or preserved host.
    Enrolled,
    /// `agent.toml` and no `identity\`.
    Pending,
    /// Neither.
    Fresh,
}

/// What preflight is asked: a bootstrap file, either migration variable, and
/// the setup's full window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Asked {
    pub bootstrap: bool,
    pub migration: bool,
    pub full_window: bool,
}

/// The states the wizard shows on (point 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offer {
    /// A fresh host: the wizard is the input item 3 requires.
    Fresh,
    /// A pending host: the wizard offers to keep its configuration or to
    /// replace it with new input.
    Pending,
}

/// Where item 3's table leaves a request, before any input is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// The migration route, its variables still to be read in their form.
    Migrate,
    /// The service is started with no argument; no token is taken.
    Start,
    /// The bootstrap file's input is staged.
    Stage,
    /// The wizard shows.
    Ask(Offer),
}

fn migration_only(what: &str, input: &str) -> Refusal {
    Refusal::new(
        CONSOLE_LAYOUT,
        format!(
            "{what}: only the migration route continues, with CG_MIGRATE_LEGACY and \
             CG_TRUST_ANCHORS and {input}"
        ),
    )
}

const MIGRATION_IN_PROGRESS: &str =
    "a migration is in progress, which the service completes with the arguments it recorded";

/// Item 3's table, with point 2: the state and what is asked. `console` is
/// the console layout's refusal, the service's own text.
pub fn route(kind: &Kind, asked: Asked, console: &str) -> Result<Route, Refusal> {
    let migration = |what: &str| {
        if asked.bootstrap || !asked.migration {
            Err(migration_only(what, "no bootstrap"))
        } else {
            Ok(Route::Migrate)
        }
    };
    match kind {
        Kind::Untrusted(reason) => Err(Refusal::new(
            UNTRUSTED_PATH,
            format!("an untrusted path: {reason}"),
        )),
        Kind::MigrationInProgress => migration(MIGRATION_IN_PROGRESS),
        Kind::ConsoleLayout => migration(console),
        _ if asked.migration => Err(Refusal::new(
            CONSOLE_LAYOUT,
            "a migration is requested, but this host holds no console installation to migrate",
        )),
        Kind::Enrolled if asked.bootstrap => Err(Refusal::new(
            ALREADY_CONFIGURED,
            "the agent is already configured: the data directory holds identity, an enrolled \
             or preserved agent, and a bootstrap is not taken; run the setup without one to \
             repair or upgrade it",
        )),
        Kind::Enrolled => Ok(Route::Start),
        Kind::Pending if asked.bootstrap => Ok(Route::Stage),
        Kind::Pending if asked.full_window => Ok(Route::Ask(Offer::Pending)),
        Kind::Pending => Ok(Route::Start),
        Kind::Fresh if asked.bootstrap => Ok(Route::Stage),
        Kind::Fresh if asked.full_window => Ok(Route::Ask(Offer::Fresh)),
        Kind::Fresh => Err(Refusal::new(
            NO_INPUT,
            "no input: a new installation needs a bootstrap file (CG_BOOTSTRAP) or the \
             wizard's input",
        )),
    }
}

/// What the operator chose in the wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// New input, accepted.
    Input,
    /// A pending host's configuration kept.
    Keep,
}

/// Where the second reading leaves an accepted choice (point 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Again {
    /// The input is staged.
    Stage,
    /// The service is started with no argument.
    Start,
}

/// Point 3: item 3 applied to the state read again after the dialog, as to a
/// bootstrap file's input; a choice to keep needs the host still pending.
pub fn route_again(kind: &Kind, choice: Choice, console: &str) -> Result<Again, Refusal> {
    let since = "since the wizard was shown";
    match kind {
        Kind::Untrusted(reason) => Err(Refusal::new(
            UNTRUSTED_PATH,
            format!("an untrusted path, {since}: {reason}"),
        )),
        Kind::MigrationInProgress => Err(migration_only(
            &format!("{MIGRATION_IN_PROGRESS}, {since}"),
            "no other input",
        )),
        Kind::ConsoleLayout => Err(migration_only(
            &format!("{console}, {since}"),
            "no other input",
        )),
        Kind::Enrolled => Err(Refusal::new(
            ALREADY_CONFIGURED,
            format!(
                "the agent is already configured, {since}: the data directory holds identity, \
                 an enrolled or preserved agent, and the wizard's choice is not taken; run the \
                 setup again to repair or upgrade it"
            ),
        )),
        Kind::Pending => Ok(match choice {
            Choice::Input => Again::Stage,
            Choice::Keep => Again::Start,
        }),
        Kind::Fresh => match choice {
            Choice::Input => Ok(Again::Stage),
            Choice::Keep => Err(Refusal::new(
                NO_INPUT,
                format!(
                    "no input: the configuration the wizard was to keep is gone, {since}; a \
                     new installation needs a bootstrap file (CG_BOOTSTRAP) or the wizard's input"
                ),
            )),
        },
    }
}

/// The wizard's fields (point 4), as the operator typed them. Its `Debug`
/// writes no token.
#[derive(Default)]
pub struct Fields {
    /// `server.url`.
    pub server_url: String,
    /// `server.heartbeat_url`.
    pub heartbeat_url: String,
    /// `server.trust_anchor_path`, typed.
    pub anchor_path: String,
    /// `setup.trust_anchor_sha256`, one per line.
    pub fingerprints: String,
    /// `enrollment.token`.
    pub token: Zeroizing<String>,
}

impl std::fmt::Debug for Fields {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fields")
            .field("server_url", &self.server_url)
            .field("heartbeat_url", &self.heartbeat_url)
            .field("anchor_path", &self.anchor_path)
            .field("fingerprints", &self.fingerprints)
            .field("token", &"<redacted>")
            .finish()
    }
}

impl Fields {
    /// The fingerprints' lines: each line with its line break removed, and a
    /// line left empty skipped; any other line is a value, as typed.
    pub fn fingerprint_lines(&self) -> Vec<&str> {
        self.fingerprints
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .filter(|line| !line.is_empty())
            .collect()
    }

    /// The fields' inputs, through the bootstrap file's validation: the
    /// document a bootstrap file of these values holds, checked as the file's
    /// is. The error names a field by its key, never a value.
    pub fn inputs(&self) -> Result<Inputs, String> {
        let string = |text: &str| toml::Value::String(text.to_string());
        let mut server = toml::Table::new();
        server.insert("url".into(), string(&self.server_url));
        server.insert("heartbeat_url".into(), string(&self.heartbeat_url));
        server.insert("trust_anchor_path".into(), string(&self.anchor_path));
        let mut enrollment = toml::Table::new();
        enrollment.insert("token".into(), string(&self.token));
        let mut setup = toml::Table::new();
        setup.insert(
            "trust_anchor_sha256".into(),
            toml::Value::Array(self.fingerprint_lines().into_iter().map(string).collect()),
        );
        let mut document = toml::Table::new();
        document.insert("server".into(), toml::Value::Table(server));
        document.insert("enrollment".into(), toml::Value::Table(enrollment));
        document.insert("setup".into(), toml::Value::Table(setup));
        parse_table(document)
    }

    /// Zero the token and empty it.
    pub fn clear_token(&mut self) {
        self.token.zeroize();
    }
}

/// Why the dialog closed with no accepted submission: each one is 22 with
/// nothing changed (point 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closed {
    Cancelled,
    /// 15 minutes after it showed with no accepted submission.
    TimedOut,
    /// The process that started the setup exited.
    StarterExited,
}

impl Closed {
    /// Code 22, its line naming the cause.
    pub fn refusal(self) -> Refusal {
        let cause = match self {
            Closed::Cancelled => "the wizard was cancelled",
            Closed::TimedOut => "the wizard was closed 15 minutes after it was shown",
            Closed::StarterExited => "the process that started the setup exited",
        };
        Refusal::new(NO_INPUT, format!("no input: {cause}; nothing was changed"))
    }
}

/// What the dialog answers.
#[derive(Debug)]
pub enum Answer {
    /// A submission accepted: its input, checked.
    Input(Staged),
    /// A pending host's configuration kept.
    Keep,
    Closed(Closed),
}

/// Whether the dialog shows a refusal of a submission and asks again: a
/// field (24) or the anchor (25). Any other refusal closes it (point 4).
pub fn asks_again(refusal: &Refusal) -> bool {
    matches!(refusal.code, INVALID_INPUT | ANCHOR)
}

/// The process that started the setup, held by a handle (point 3).
pub trait Starter {
    /// Whether it still runs.
    fn running(&self) -> bool;
}

/// The dialog, and the process it waits on.
pub trait Wizard {
    /// The process that started this one, held: code 40 if it cannot be,
    /// and then no dialog shows.
    fn hold_starter(&self) -> Result<Box<dyn Starter>, Refusal>;

    /// Show the dialog for `offer` until `take` accepts a submission (each
    /// refusal for which `asks_again` holds shown in it and asked again; any
    /// other closing it, returned as `Err`), a choice to keep, a
    /// cancellation, 15 minutes, or `starter`'s exit.
    fn ask(
        &self,
        offer: Offer,
        starter: &dyn Starter,
        take: &mut dyn FnMut(&Fields) -> Result<Staged, Refusal>,
    ) -> Result<Answer, Refusal>;
}
