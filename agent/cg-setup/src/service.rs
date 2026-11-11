//! The product service as the setup reads and starts it (SPEC-024
//! Amendment 2026-11-10, items 3, 6 and 8): when a service's configuration is
//! the product's, the start arguments, and the failure actions. The service
//! control manager itself is `scm`'s; what is here reads no host state.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use cg_agent::service::is_fixed_path;

/// The product service's name (§Data contracts §3).
pub const PRODUCT_SERVICE: &str = "CyberGuardAgent";

/// The product service's account (§Data contracts §3).
pub const LOCAL_SYSTEM: &str = "LocalSystem";

/// `SERVICE_WIN32_OWN_PROCESS`, with no other bit.
pub const OWN_PROCESS: u32 = 0x10;

/// How long preflight has to prove the service `STOPPED`, from its first
/// stop request (item 6): longer than the service's 60-second deadline.
pub const STOP_WITHIN: Duration = Duration::from_secs(75);

/// The failure actions (the Amendment 2026-11-09, item 8): a restart after
/// each of these delays, in milliseconds.
pub const RESTART_DELAYS_MS: [u32; 3] = [60_000, 120_000, 300_000];

/// The failure count resets after a day without failure, in seconds.
pub const FAILURE_RESET_SECONDS: u32 = 86_400;

/// The start argument that asks for the adoption (the Amendment 2026-11-09,
/// item 6).
pub const ADOPT: &str = "--adopt-staged-config";

/// A service's configuration, as the SCM reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Configuration {
    pub service_type: u32,
    pub account: String,
    pub command_line: String,
}

/// The product's command line (§Data contracts §3): the quoted absolute
/// binary path, `--service`, and `--config` with the quoted config path.
pub fn product_command_line(binary: &Path, config: &Path) -> String {
    format!(
        "\"{}\" --service --config \"{}\"",
        binary.display(),
        config.display()
    )
}

/// The two paths of a command line of the product's form, or `None` when it
/// is not of that form.
fn command_line_paths(command_line: &str) -> Option<(&str, &str)> {
    let rest = command_line.strip_prefix('"')?;
    let (binary, rest) = rest.split_once('"')?;
    let config = rest
        .strip_prefix(" --service --config \"")?
        .strip_suffix('"')?;
    (!config.contains('"')).then_some((binary, config))
}

/// Item 3: why `found` is not the product service's configuration, or
/// `None` when it is. Its type is exactly own process, its account
/// `LocalSystem`, and its command line the product's, the two paths in it
/// equal to `binary` and `config` but for ASCII case, as the service compares
/// them (the Amendment 2026-11-04, item 2), and the rest exactly. The start
/// type is not compared. The reason quotes no value of the configuration.
pub fn not_the_products(found: &Configuration, binary: &Path, config: &Path) -> Option<String> {
    if found.service_type != OWN_PROCESS {
        return Some(format!(
            "its type is {:#x}, not own process alone",
            found.service_type
        ));
    }
    // An account name is not case-sensitive.
    if !found.account.eq_ignore_ascii_case(LOCAL_SYSTEM) {
        return Some("its account is not LocalSystem".to_string());
    }
    match command_line_paths(&found.command_line) {
        Some((found_binary, found_config))
            if is_fixed_path(found_binary, binary) && is_fixed_path(found_config, config) =>
        {
            None
        }
        _ => Some("its command line is not the product's".to_string()),
    }
}

/// Configure's start arguments on a route other than a migration's (item 8):
/// with the record's fingerprints, the adoption's (the Amendment 2026-11-09,
/// item 6), `--adopt-staged-config` once, then each fingerprint; with an
/// empty list, none.
pub fn start_arguments(fingerprints: &BTreeSet<String>) -> Vec<String> {
    if fingerprints.is_empty() {
        return Vec::new();
    }
    std::iter::once(ADOPT.to_string())
        .chain(
            fingerprints
                .iter()
                .map(|fingerprint| format!("--trust-anchor-sha256={fingerprint}")),
        )
        .collect()
}

/// The start arguments of the migration (the Amendment 2026-11-05, item 1).
#[cfg(windows)]
pub fn migration_arguments(request: &cg_agent::service::migration::Request) -> Vec<String> {
    use cg_agent::service::migration::Choice;
    let choice = match request.choice {
        Choice::Remove => "remove",
        Choice::Keep => "keep",
    };
    std::iter::once(format!("--migrate-legacy={choice}"))
        .chain(
            request
                .fingerprints
                .iter()
                .map(|fingerprint| format!("--trust-anchor-sha256={fingerprint}")),
        )
        .collect()
}
