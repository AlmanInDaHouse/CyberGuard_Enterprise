//! `cg-setup`, the setup's elevated code (SPEC-024 Amendment 2026-11-09,
//! item 1): what preflight reads of its inputs and of the host before
//! anything changes it (items 2 and 3), the configuration it stages, and the
//! exit codes the operator reads (item 7); the staging, its record and its
//! reversal, and the client of the service control manager that checks,
//! stops, configures and starts the product service, and the installed
//! binary's version (the Amendment 2026-11-10, items 3 to 8, but for configure's
//! observer).
//!
//! It enables no privilege and does not adjust its token (ADR-0010
//! §Compliance). The host's state is the service's own reading,
//! `cg_agent::service::layout::classify`, with the service's check of the
//! data directories, and the configuration it generates passes the checks the
//! service's adoption applies to it, through the same code.

pub mod codes;
pub mod inputs;
#[cfg(windows)]
pub mod observer;
pub mod platform;
#[cfg(windows)]
pub mod preflight;
pub mod record;
#[cfg(windows)]
pub mod scm;
pub mod service;
pub mod staged;
#[cfg(windows)]
pub mod staging;
pub mod version;

pub use codes::Refusal;
