//! `cg-setup.exe`, the setup's elevated code (SPEC-024 Amendment 2026-11-09,
//! item 1), as Burn runs it (the Amendment 2026-11-10, item 9): one verb,
//! `preflight`, `configure` or `reverse`, or `--version`. The platform's floor
//! is checked first (item 1), then the command line, then, for each verb, the
//! elevation; the verb then runs in its item's order (`cg_setup::verbs`).
//! The exit code is the operator's result (the Amendment 2026-11-09, item
//! 7); a refusal or failure is one line on standard error, which never holds
//! the token or a line of an input file.

use std::process::ExitCode;

use cg_setup::Refusal;

fn main() -> ExitCode {
    let arguments: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    match run(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        // A line with code 0 is configure's record left behind an
        // operational agent (the Amendment 2026-11-11 (second), point 4).
        Err(refusal) => {
            eprintln!("{}", refusal.line);
            ExitCode::from(refusal.code)
        }
    }
}

#[cfg(windows)]
fn run(arguments: &[std::ffi::OsString]) -> Result<(), Refusal> {
    use cg_setup::cli::{parse, Command};
    use cg_setup::platform::{check_elevation, check_platform};
    use cg_setup::verbs::{self, Machine};
    use cg_setup::version::SETUP_VERSION;

    // The floor is read before any other check (item 1): a command line Burn
    // did not form is refused after it, and `--version` answers anywhere.
    let command = parse(arguments);
    if command == Ok(Command::Version) {
        println!("cg-setup {SETUP_VERSION}");
        return Ok(());
    }
    check_platform()?;
    let command = command?;
    check_elevation()?;
    let machine = Machine::system()?;
    match command {
        Command::Preflight(request) => verbs::preflight(&machine, &request),
        Command::Configure(request) => verbs::configure(&machine, &request),
        Command::Reverse => verbs::reverse(&machine),
        Command::Version => Ok(()),
    }
}

/// Off Windows the setup has nothing to run: the platform's 20.
#[cfg(not(windows))]
fn run(_: &[std::ffi::OsString]) -> Result<(), Refusal> {
    cg_setup::platform::check_platform()
}
