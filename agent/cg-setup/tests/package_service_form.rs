//! What the setup compares and passes of the product service, with no service
//! control manager (SPEC-024 Amendment 2026-11-10, items 3, 6 and 8): when a
//! configuration is the product's, the start arguments, the stop's bound and
//! the failure actions. The SCM itself is tested in `package_setup_service`.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use cg_setup::service::{
    not_the_products, product_command_line, start_arguments, Configuration, FAILURE_RESET_SECONDS,
    LOCAL_SYSTEM, OWN_PROCESS, PRODUCT_SERVICE, RESTART_DELAYS_MS, STOP_WITHIN,
};

const BINARY: &str = r"C:\Program Files\CyberGuard\Agent\cg-agent.exe";
const CONFIG: &str = r"C:\ProgramData\CyberGuard\agent\agent.toml";

fn products() -> Configuration {
    Configuration {
        service_type: OWN_PROCESS,
        account: LOCAL_SYSTEM.to_string(),
        command_line: format!("\"{BINARY}\" --service --config \"{CONFIG}\""),
    }
}

fn reason(found: &Configuration) -> Option<String> {
    not_the_products(found, Path::new(BINARY), Path::new(CONFIG))
}

fn fingerprint(digit: char) -> String {
    format!("sha256:{}", digit.to_string().repeat(64))
}

#[test]
fn package_service_the_product_service_and_its_command_line() {
    assert_eq!(PRODUCT_SERVICE, "CyberGuardAgent");
    assert_eq!(
        product_command_line(Path::new(BINARY), Path::new(CONFIG)),
        format!("\"{BINARY}\" --service --config \"{CONFIG}\""),
        "the quoted binary, --service, and --config with the quoted config path"
    );
    assert_eq!(reason(&products()), None, "the product's own configuration");
}

#[test]
fn package_service_the_paths_compare_but_for_ascii_case() {
    let mut found = products();
    found.command_line = format!(
        "\"{}\" --service --config \"{}\"",
        BINARY.to_ascii_uppercase(),
        CONFIG.to_ascii_lowercase()
    );
    assert_eq!(
        reason(&found),
        None,
        "the two paths equal but for ASCII case"
    );

    // A non-ASCII letter's case is not folded.
    let binary = r"C:\Program Files\CyberGuard\Agént\cg-agent.exe";
    let found = Configuration {
        command_line: format!(
            "\"{}\" --service --config \"{CONFIG}\"",
            binary.to_uppercase()
        ),
        ..products()
    };
    assert!(
        not_the_products(&found, Path::new(binary), Path::new(CONFIG)).is_some(),
        "É is not é"
    );
}

#[test]
fn package_service_the_type_must_be_own_process_alone() {
    for service_type in [0x20, 0x10 | 0x100, 0x110, 0x01, 0] {
        let found = Configuration {
            service_type,
            ..products()
        };
        let reason = reason(&found).unwrap_or_else(|| panic!("type {service_type:#x}"));
        assert!(reason.contains("type"), "{reason}");
    }
}

#[test]
fn package_service_the_account_is_local_system_in_any_ascii_case() {
    for account in ["LocalSystem", "localsystem", "LOCALSYSTEM"] {
        let found = Configuration {
            account: account.to_string(),
            ..products()
        };
        assert_eq!(reason(&found), None, "{account}");
    }
    for account in [
        "",
        r"NT AUTHORITY\LocalService",
        r"NT AUTHORITY\NetworkService",
        r".\LocalSystem",
        "LocalSystem ",
        r"NT AUTHORITY\SYSTEM",
    ] {
        let found = Configuration {
            account: account.to_string(),
            ..products()
        };
        let reason = reason(&found).unwrap_or_else(|| panic!("{account:?}"));
        assert!(reason.contains("account"), "{reason}");
        assert!(
            account.is_empty() || !reason.contains(account),
            "the reason quotes no value: {reason}"
        );
    }
}

#[test]
fn package_service_the_rest_of_the_command_line_is_exact() {
    for command_line in [
        format!("{BINARY} --service --config \"{CONFIG}\""),
        format!("\"{BINARY}\" --service --config {CONFIG}"),
        format!("\"{BINARY}\" --service --config \"{CONFIG}\" "),
        format!(" \"{BINARY}\" --service --config \"{CONFIG}\""),
        format!("\"{BINARY}\"  --service --config \"{CONFIG}\""),
        format!("\"{BINARY}\" --SERVICE --config \"{CONFIG}\""),
        format!("\"{BINARY}\" --config \"{CONFIG}\" --service"),
        format!("\"{BINARY}\" --service --config \"{CONFIG}\" --adopt-staged-config"),
        format!("\"{BINARY}\" --service --config \"{CONFIG}\"\"\""),
        format!("\"{BINARY}\" --config \"{CONFIG}\""),
        format!("\"{BINARY}x\" --service --config \"{CONFIG}\""),
        format!("\"{BINARY}\" --service --config \"{CONFIG}.setup\""),
        format!("\"{CONFIG}\" --service --config \"{BINARY}\""),
        String::new(),
    ] {
        let found = Configuration {
            command_line: command_line.clone(),
            ..products()
        };
        let reason = reason(&found).unwrap_or_else(|| panic!("{command_line:?}"));
        assert!(reason.contains("command line"), "{reason}");
        assert!(
            !reason.contains(BINARY) && !reason.contains(CONFIG),
            "the reason quotes no value: {reason}"
        );
    }
}

#[test]
fn package_service_the_start_arguments() {
    let fingerprints: BTreeSet<String> = [fingerprint('b'), fingerprint('a')].into();
    assert_eq!(
        start_arguments(&fingerprints),
        vec![
            "--adopt-staged-config".to_string(),
            format!("--trust-anchor-sha256={}", fingerprint('a')),
            format!("--trust-anchor-sha256={}", fingerprint('b')),
        ],
        "--adopt-staged-config once, then each fingerprint"
    );
    assert_eq!(
        start_arguments(&BTreeSet::new()),
        Vec::<String>::new(),
        "with an empty list, the service is started with none"
    );
}

#[cfg(windows)]
#[test]
fn package_service_the_migration_arguments() {
    use cg_agent::service::migration::{parse_start, Choice, Request, Start};
    use cg_setup::service::migration_arguments;
    for choice in [Choice::Remove, Choice::Keep] {
        let request = Request {
            choice,
            fingerprints: [fingerprint('c'), fingerprint('d')].into(),
        };
        let arguments = migration_arguments(&request);
        assert_eq!(
            arguments[0],
            match choice {
                Choice::Remove => "--migrate-legacy=remove",
                Choice::Keep => "--migrate-legacy=keep",
            }
        );
        assert_eq!(
            parse_start(&arguments),
            Ok(Start::Migrate(request)),
            "the service reads them as the migration they ask for"
        );
    }
    let fingerprints: BTreeSet<String> = [fingerprint('e')].into();
    assert_eq!(
        parse_start(&start_arguments(&fingerprints)),
        Ok(Start::Adopt(fingerprints)),
        "the service reads the adoption's arguments as the adoption"
    );
    assert_eq!(
        parse_start(&start_arguments(&BTreeSet::new())),
        Ok(Start::Plain),
        "and an empty list's as a start that adopts nothing"
    );
}

#[test]
fn package_service_the_stop_bound_and_the_failure_actions() {
    assert_eq!(STOP_WITHIN, Duration::from_secs(75));
    assert_eq!(RESTART_DELAYS_MS, [60_000, 120_000, 300_000]);
    assert_eq!(FAILURE_RESET_SECONDS, 86_400);
}
