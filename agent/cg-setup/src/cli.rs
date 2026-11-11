//! `cg-setup`'s command line (SPEC-024 Amendment 2026-11-10, item 9): Burn
//! runs it with one verb, `preflight`, `configure` or `reverse`, and the
//! bundle variables of the Amendment 2026-11-09, item 2, as options of the
//! form `--name=value`, each at most once, an empty value as not given.
//! `--version` answers the setup's version. Any other command line is 40.
//!
//! A refusal names the verb and an option's name, never a value: a value is
//! a bundle variable, which carries no secret, but a misplaced one could.

use std::ffi::OsString;

use crate::codes::{Refusal, OWN_STATE};
use crate::preflight::Request;

/// What the command line asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `cg-setup --version`.
    Version,
    /// `cg-setup preflight [--bootstrap=] [--migrate-legacy=] [--trust-anchors=]`.
    Preflight(Request),
    /// `cg-setup configure [--migrate-legacy=] [--trust-anchors=]`.
    Configure(Request),
    /// `cg-setup reverse`.
    Reverse,
}

const BOOTSTRAP: &str = "--bootstrap";
const MIGRATE_LEGACY: &str = "--migrate-legacy";
const TRUST_ANCHORS: &str = "--trust-anchors";

fn refused(reason: impl std::fmt::Display) -> Refusal {
    Refusal::new(
        OWN_STATE,
        format!("the command line is not one the setup's bundle forms: {reason}"),
    )
}

/// The command of `arguments`, those after the program's name.
pub fn parse(arguments: &[OsString]) -> Result<Command, Refusal> {
    let arguments = arguments
        .iter()
        .enumerate()
        .map(|(index, argument)| {
            argument
                .to_str()
                .ok_or_else(|| refused(format!("argument {} is not Unicode", index + 1)))
        })
        .collect::<Result<Vec<&str>, Refusal>>()?;
    let Some((&verb, options)) = arguments.split_first() else {
        return Err(refused("no verb"));
    };
    match verb {
        "--version" if options.is_empty() => Ok(Command::Version),
        "--version" => Err(refused("--version takes nothing")),
        "reverse" if options.is_empty() => Ok(Command::Reverse),
        "reverse" => Err(refused("reverse takes nothing")),
        "preflight" => {
            let [bootstrap, migrate_legacy, trust_anchors] =
                take(verb, options, [BOOTSTRAP, MIGRATE_LEGACY, TRUST_ANCHORS])?;
            Ok(Command::Preflight(Request::new(
                bootstrap,
                migrate_legacy,
                trust_anchors,
            )))
        }
        "configure" => {
            let [migrate_legacy, trust_anchors] =
                take(verb, options, [MIGRATE_LEGACY, TRUST_ANCHORS])?;
            Ok(Command::Configure(Request::new(
                None,
                migrate_legacy,
                trust_anchors,
            )))
        }
        _ => Err(refused(
            "the first argument is not preflight, configure, reverse or --version",
        )),
    }
}

/// The value of each option `names` admits, from `options`, each given at
/// most once; any other argument is refused.
fn take<'a, const N: usize>(
    verb: &str,
    options: &[&'a str],
    names: [&str; N],
) -> Result<[Option<&'a str>; N], Refusal> {
    let mut values = [None; N];
    for (index, option) in options.iter().enumerate() {
        let position = index + 2;
        let Some((name, value)) = option.split_once('=') else {
            return Err(refused(format!(
                "argument {position} is not an option of the form --name=value {verb} takes"
            )));
        };
        let Some(slot) = names.iter().position(|known| *known == name) else {
            return Err(refused(format!(
                "argument {position} is not an option {verb} takes"
            )));
        };
        if values[slot].replace(value).is_some() {
            return Err(refused(format!("{name}= is given more than once")));
        }
    }
    Ok(values)
}
