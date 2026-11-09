//! The service's adoption of the staged configuration (SPEC-024 Amendment
//! 2026-11-09, item 6): the service, as SYSTEM, during `START_PENDING`,
//! makes the configuration and the trust anchor the setup staged its own.
//!
//! `adopt` runs before the start's other steps, only when the SCM start
//! arguments ask for it. It reads each staged file once, through a handle
//! that excludes writers and that it holds until step 6, closes `<P>` and the
//! data root as the migration does, and commits by two renames, the
//! configuration last. Whatever stops it leaves a state the next setup can
//! retry from; the `Environment` is the migration's, so that the tests point
//! it at a temporary root and inject a failure and a crash at each `Point`.

use std::collections::BTreeSet;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{DELETE, FILE_READ_ATTRIBUTES};

use super::layout::Names;
use super::migration::{
    check_anchor, close, counts, identity_line, in_place, invalid_arguments, naming,
    other_combination, refused, Environment, Injected, Point, Stop, READ_LIMIT, SHARE_ALL,
};
use super::{
    check_installed_config, config_error_line, stderr_service_log, InstalledPaths, EXIT_SERVICE_LOG,
};
use crate::custody::{self, Handle, Policy};
use crate::errors::{ConfigError, TlsError};

const STAGED_CONFIG: &str = "agent.toml.setup";
const STAGED_ANCHOR: &str = "server-ca.pem.setup";
const CONFIG: &str = "agent.toml";
const ANCHOR: &str = "server-ca.pem";

/// A cut step's stop: `render` makes the refusal of its reason.
fn cut_short(cut: Injected, render: impl FnOnce(&str) -> Stop) -> Stop {
    match cut {
        Injected::Crash => Stop::Crashed,
        Injected::Failure(reason) => render(&reason),
    }
}

fn anchor_line(reason: &str) -> String {
    TlsError::ClientConfig(format!("adoption: {reason}")).stderr_line()
}

/// Code 13 for `<P>` and the data root, the file log's reason.
fn directories_line(reason: &str) -> Stop {
    refused(EXIT_SERVICE_LOG, stderr_service_log(reason))
}

/// Item 6.1, read again under the lock: a console layout or a migration in
/// progress refuses these arguments, before any change.
fn read_names(environment: &Environment<'_>) -> Result<Names, Stop> {
    let names = Names::read(&environment.paths, environment.wait)
        .map_err(|reason| directories_line(&reason))?;
    let wrong_host = || {
        let line = invalid_arguments(
            "--adopt-staged-config is not accepted on a console layout or a migration in progress",
        );
        // The Amendment 2026-11-07 (second), item 5: beside a console layout
        // with a staging directory that counts, the refusal says what is
        // in place, as a migration's own does.
        if names.console_layout()
            && counts(environment, &environment.paths.staging())
            && !counts(environment, &environment.paths.legacy())
        {
            in_place(environment, 2, line)
        } else {
            refused(2, line)
        }
    };
    if names.migration_in_progress() {
        return Err(wrong_host());
    }
    // A data root that cannot be listed may be a console layout.
    if let Some(reason) = names.unlisted() {
        return Err(directories_line(reason));
    }
    if names.console_layout() {
        return Err(wrong_host());
    }
    Ok(names)
}

/// What item 6.4 asks of a staged file: `agent.toml`'s, with Administrators
/// as a possible owner, the setup's.
fn staged_policy(environment: &Environment<'_>) -> Policy {
    Policy {
        owners: environment.custody.owners.clone(),
        allowed: environment.custody.configuration.allowed.clone(),
        protected: true,
    }
}

/// Read the staged file `name` through a handle held from here on.
fn read_staged(
    environment: &Environment<'_>,
    path: &Path,
    name: &'static str,
) -> Result<Option<(Handle, Vec<u8>)>, Injected> {
    environment.watch.at(Point::AdoptRead(name))?;
    Ok(custody::read_held(
        path,
        READ_LIMIT,
        &staged_policy(environment),
        environment.wait,
    )?)
}

/// The staged configuration's own checks: its line when it is refused, which
/// names the file and quotes nothing of it.
fn check_staged_config(paths: &InstalledPaths, path: &Path, bytes: Vec<u8>) -> Result<(), String> {
    let invalid = |reason: &str| format!("cg-agent: invalid config {}: {reason}", path.display());
    let text = String::from_utf8(bytes).map_err(|_| invalid("the file is not UTF-8"))?;
    let config = crate::config::load_from_str(&text).map_err(|e| config_error_line(path, &e))?;
    let document: toml::Value = text
        .parse()
        .map_err(|_| invalid("the file is not valid TOML"))?;
    if document.get("setup").is_some() {
        return Err(invalid("a staged configuration holds no setup table"));
    }
    check_installed_config(paths, path, &config)?;
    let has_token = config
        .enrollment
        .as_ref()
        .and_then(|enrollment| enrollment.token.as_deref())
        .is_some_and(|token| !token.is_empty());
    if !has_token {
        return Err(invalid("the staged configuration has no enrollment token"));
    }
    Ok(())
}

/// Item 6.4 for the configuration (code 2).
fn hold_config(environment: &Environment<'_>) -> Result<Handle, Stop> {
    let path = environment.paths.staged_config();
    let invalid = |reason: &str| {
        refused(
            2,
            format!("cg-agent: invalid config {}: {reason}", path.display()),
        )
    };
    let (handle, bytes) = match read_staged(environment, &path, STAGED_CONFIG) {
        Ok(Some(held)) => held,
        Ok(None) => return Err(no_staged_configuration(&path)),
        Err(cut) => return Err(cut_short(cut, invalid)),
    };
    check_staged_config(&environment.paths, &path, bytes).map_err(|line| refused(2, line))?;
    Ok(handle)
}

fn no_staged_configuration(path: &Path) -> Stop {
    refused(
        2,
        config_error_line(path, &ConfigError::NotFound(path.display().to_string())),
    )
}

/// Item 6.4 for the anchor (code 6).
fn hold_anchor(
    environment: &Environment<'_>,
    fingerprints: &BTreeSet<String>,
) -> Result<Handle, Stop> {
    let path = environment.paths.staged_anchor();
    let invalid = |reason: &str| refused(6, anchor_line(reason));
    let (handle, bytes) = match read_staged(environment, &path, STAGED_ANCHOR) {
        Ok(Some(held)) => held,
        Ok(None) => return Err(invalid(&format!("{} does not exist", path.display()))),
        Err(cut) => return Err(cut_short(cut, invalid)),
    };
    check_anchor(&bytes, fingerprints).map_err(|reason| invalid(&reason))?;
    Ok(handle)
}

/// Item 6.5 for one staged file: the owner and DACL through the held handle,
/// and its link count read again.
fn restrict(
    environment: &Environment<'_>,
    path: &Path,
    name: &'static str,
    handle: Handle,
) -> Result<Handle, Injected> {
    environment.watch.at(Point::AdoptRestrict(name))?;
    let handle = custody::set_security(
        handle,
        path,
        &environment.custody.shared_file_sddl,
        environment.wait,
    )?;
    environment.watch.at(Point::AdoptRestricted(name))?;
    if custody::file_information(&handle, path)?.nNumberOfLinks != 1 {
        return Err(format!("{} is hard-linked", path.display()).into());
    }
    Ok(handle)
}

/// Item 6.6's deletion: whatever is at `path`, through a handle, a reparse
/// point not followed.
fn delete_final(
    environment: &Environment<'_>,
    path: &Path,
    name: &'static str,
) -> Result<(), Injected> {
    environment.watch.at(Point::AdoptDelete(name))?;
    let handle = custody::open_existing(
        path,
        DELETE | FILE_READ_ATTRIBUTES,
        SHARE_ALL,
        environment.wait,
    )?;
    if let Some(handle) = handle {
        custody::delete_through(handle, path, environment.wait)?;
    }
    Ok(())
}

/// Item 6.6: the held handles close before the renames, which they would
/// otherwise share-exclude.
fn commit(environment: &Environment<'_>, config: Handle, anchor: Handle) -> Result<(), Injected> {
    let paths = &environment.paths;
    environment.watch.at(Point::AdoptNames)?;
    let names = Names::read(paths, environment.wait)?;
    if let Some(reason) = names.unlisted() {
        return Err(reason.to_string().into());
    }
    if names.holds_identity() {
        return Err(format!(
            "{} holds the name identity: adoption never points an enrolled identity at new inputs",
            paths.data_root.display()
        )
        .into());
    }
    delete_final(environment, &paths.config, CONFIG)?;
    delete_final(environment, &paths.trust_anchor, ANCHOR)?;
    drop((config, anchor));
    environment.watch.at(Point::AdoptRename(ANCHOR))?;
    custody::rename(
        &paths.staged_anchor(),
        &paths.trust_anchor,
        environment.wait,
    )?;
    environment.watch.at(Point::AdoptRename(CONFIG))?;
    custody::rename(&paths.staged_config(), &paths.config, environment.wait)?;
    Ok(())
}

/// Item 6: adopt the staged configuration. `lock` takes the instance lock; it
/// is called before anything changes, and once.
pub fn adopt(
    environment: &Environment<'_>,
    fingerprints: &BTreeSet<String>,
    lock: &mut dyn FnMut() -> Result<(), Stop>,
) -> Result<(), Stop> {
    let paths = &environment.paths;
    read_names(environment)?;
    lock()?;
    let names = read_names(environment)?;

    // Item 2.1's condition, met before anything is read through it.
    if let Some(path) = names.reparse_point(paths) {
        return Err(directories_line(&format!(
            "{} is a reparse point",
            path.display()
        )));
    }
    let code_5 = |reason: &str| refused(5, identity_line(reason));
    if names.holds_identity() {
        return Err(code_5(&format!(
            "{} holds the name identity: adoption never points an enrolled identity at new inputs; nothing was changed",
            paths.data_root.display()
        )));
    }
    if names.in_data_root(STAGED_CONFIG).is_none() {
        return Err(no_staged_configuration(&paths.staged_config()));
    }
    // The Amendment 2026-11-05, item 4: a data root beside a legacy directory
    // that counts, with no identity, is no state a migration leaves, and
    // nothing is changed.
    if counts(environment, &paths.legacy()) {
        return Err(other_combination(environment));
    }

    let config = hold_config(environment)?;
    let anchor = hold_anchor(environment, fingerprints)?;

    for (path, point) in [
        (&paths.product_root, Point::CloseProductRoot),
        (&paths.data_root, Point::CloseDataRoot),
    ] {
        close(environment, path, point)
            .map_err(|cut| cut_short(naming(path, cut), directories_line))?;
    }
    let config = restrict(environment, &paths.staged_config(), STAGED_CONFIG, config)
        .map_err(|cut| cut_short(cut, code_5))?;
    let anchor = restrict(environment, &paths.staged_anchor(), STAGED_ANCHOR, anchor)
        .map_err(|cut| cut_short(cut, code_5))?;
    commit(environment, config, anchor).map_err(|cut| cut_short(cut, code_5))
}
