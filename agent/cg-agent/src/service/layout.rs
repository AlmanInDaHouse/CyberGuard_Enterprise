//! The read-only classification of the host's state (SPEC-024 Amendment
//! 2026-11-09, item 3): what preflight reads before anything changes the host,
//! and what the service reads before it adopts (item 6.1).
//!
//! Everything here is read by name, from the listings of `<P>` and the data
//! root: no object under them is opened, so `identity\` is not, and an
//! opportunistic lock on a name costs nothing. `<P>` itself is opened without
//! following a reparse point, and neither it nor the data root is listed when
//! it is one. A name an ordinary user planted yields at most a refusal: each
//! later step checks what it uses through a handle of its own.

use std::path::Path;
use std::time::Duration;

use super::InstalledPaths;
use crate::custody::{self, Kind, Policy, Shape};

const DATA_ROOT: &str = "agent";
const STAGING: &str = "agent.migrating";
const LEGACY: &str = "agent.legacy";
const IDENTITY_DIRECTORY: &str = "identity";
const CONFIG: &str = "agent.toml";
/// What makes a data root a console layout (Amendment 2026-11-05).
const CONSOLE_FILES: [&str; 3] = ["cert.pem", "key.dat", "identity.json"];

/// The state of the host (item 3), the first that applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostState {
    /// A reparse point, or what fails the directories' check; why, naming a
    /// path and the failed condition.
    Untrusted(String),
    /// A directory `agent.migrating`, or `agent.legacy` with no data root.
    MigrationInProgress,
    ConsoleLayout,
    /// `identity\` is there: an enrolled or preserved host.
    Enrolled,
    /// `agent.toml` and no `identity\`.
    Pending,
    /// Neither.
    Fresh,
}

/// What the listings show, for one reading of the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Names {
    product_root: Option<Shape>,
    data_root: Option<Shape>,
    in_product_root: Vec<(String, Shape)>,
    in_data_root: Vec<(String, Shape)>,
    /// Why the data root's listing failed: the earlier states still apply.
    unlisted: Option<String>,
}

fn lookup(listing: &[(String, Shape)], name: &str) -> Option<Shape> {
    listing
        .iter()
        .find(|(entry, _)| entry.eq_ignore_ascii_case(name))
        .map(|(_, shape)| *shape)
}

impl Names {
    /// Read `<P>` and, when it is a directory, its names and the data root's.
    pub fn read(paths: &InstalledPaths, wait: Duration) -> Result<Self, String> {
        let product_root = custody::shape(&paths.product_root, wait)?;
        let mut names = Names {
            product_root,
            data_root: None,
            in_product_root: Vec::new(),
            in_data_root: Vec::new(),
            unlisted: None,
        };
        if product_root != Some(Shape::Directory) {
            return Ok(names);
        }
        names.in_product_root = custody::listing(&paths.product_root, wait)?;
        names.data_root = lookup(&names.in_product_root, DATA_ROOT);
        if names.data_root == Some(Shape::Directory) {
            match custody::listing(&paths.data_root, wait) {
                Ok(listing) => names.in_data_root = listing,
                Err(reason) => names.unlisted = Some(reason),
            }
        }
        Ok(names)
    }

    /// `<P>` or the data root is a reparse point.
    pub fn reparse_point<'a>(&self, paths: &'a InstalledPaths) -> Option<&'a Path> {
        if self.product_root == Some(Shape::ReparsePoint) {
            return Some(&paths.product_root);
        }
        (self.data_root == Some(Shape::ReparsePoint)).then_some(paths.data_root.as_path())
    }

    /// Why the data root could not be listed, if it could not.
    pub fn unlisted(&self) -> Option<&str> {
        self.unlisted.as_deref()
    }

    /// A directory `agent.migrating`, or `agent.legacy` with no data root.
    pub fn migration_in_progress(&self) -> bool {
        lookup(&self.in_product_root, STAGING) == Some(Shape::Directory)
            || (self.has_legacy() && self.data_root.is_none())
    }

    /// The data root holds an identity file directly.
    pub fn console_layout(&self) -> bool {
        CONSOLE_FILES
            .iter()
            .any(|name| self.in_data_root(name).is_some())
    }

    /// The name `identity` is in the data root, whatever it is.
    pub fn holds_identity(&self) -> bool {
        self.in_data_root(IDENTITY_DIRECTORY).is_some()
    }

    /// What the name `name` is in the data root, if it is there.
    pub fn in_data_root(&self, name: &str) -> Option<Shape> {
        lookup(&self.in_data_root, name)
    }

    fn has_legacy(&self) -> bool {
        lookup(&self.in_product_root, LEGACY).is_some()
    }
}

/// The first state of item 3 that applies. `directories` is the service's
/// check of `<P>` and the data root (Amendment 2026-11-04, item 3). What cannot
/// be read is untrusted: nothing can be said of the host.
pub fn classify(paths: &InstalledPaths, directories: &Policy, wait: Duration) -> HostState {
    let names = match Names::read(paths, wait) {
        Ok(names) => names,
        Err(reason) => return HostState::Untrusted(reason),
    };
    if let Some(path) = names.reparse_point(paths) {
        return HostState::Untrusted(format!("{} is a reparse point", path.display()));
    }
    if names.migration_in_progress() {
        return HostState::MigrationInProgress;
    }
    if let Some(reason) = names.unlisted() {
        return HostState::Untrusted(reason.to_string());
    }
    if names.console_layout() {
        return HostState::ConsoleLayout;
    }
    for (path, shape) in [
        (&paths.product_root, names.product_root),
        (&paths.data_root, names.data_root),
    ] {
        if shape == Some(Shape::Other) {
            return HostState::Untrusted(format!("{} is not a directory", path.display()));
        }
        if let Err(reason) =
            custody::check_if_exists_within(path, Kind::Directory, directories, wait)
        {
            return HostState::Untrusted(reason);
        }
    }
    if names.has_legacy() && !names.holds_identity() {
        return HostState::Untrusted(format!(
            "{} is beside a data root with no identity",
            paths.legacy().display()
        ));
    }
    if names.holds_identity() {
        HostState::Enrolled
    } else if names.in_data_root(CONFIG).is_some() {
        HostState::Pending
    } else {
        HostState::Fresh
    }
}
