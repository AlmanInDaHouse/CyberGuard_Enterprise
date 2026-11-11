//! The setup's client of the service control manager against real services
//! (SPEC-024 Amendment 2026-11-10, items 3, 6 and 8): the tests are in
//! `setup_service`. This target has no libtest harness: the SCM starts this
//! same executable as the service the tests control (`--service --config
//! <file>`), and otherwise it runs the tests, one at a time.

#[cfg(windows)]
mod setup_service;

fn main() {
    #[cfg(windows)]
    setup_service::main();
}
