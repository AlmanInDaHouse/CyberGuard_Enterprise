//! SPEC-001 AC-009 — All stdout lines emitted by the agent during a
//! normal run are valid JSON, each containing `timestamp`, `level`,
//! and `message` fields.
//!
//! Tested against a `Write` sink injected into the tracing
//! subscriber instead of capturing real stdout. The implementation
//! commit exposes `init_logger_with_writer(level, writer)` so this
//! test can inspect the produced lines deterministically.

mod common;

use cg_agent::{init_logger_with_writer, log_lifecycle_event};
use serde_json::Value;
use std::io::Write;
use std::sync::{Arc, Mutex};

/// Thread-safe shared buffer that satisfies `std::io::Write` and
/// `tracing_subscriber::fmt::MakeWriter`.
#[derive(Clone, Default)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn ac_009_stdout_lines_are_json() {
    let buf = SharedBuffer::default();
    init_logger_with_writer("info", buf.clone()).expect("logger should initialise");

    // Emit a representative lifecycle event from the agent's surface.
    log_lifecycle_event("agent starting", "ac-009-test");

    let bytes = buf.0.lock().unwrap().clone();
    let text = String::from_utf8(bytes).expect("logger output is UTF-8");
    let mut any_line = false;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        any_line = true;
        let parsed: Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("not JSON: {line:?} ({e})"));
        assert!(
            parsed.get("timestamp").is_some(),
            "missing 'timestamp' field in: {line}"
        );
        assert!(
            parsed.get("level").is_some(),
            "missing 'level' field in: {line}"
        );
        assert!(
            parsed.get("message").is_some() || parsed.get("fields").is_some(),
            "missing 'message'/'fields' in: {line}"
        );
    }
    assert!(any_line, "no log line was produced");
}
