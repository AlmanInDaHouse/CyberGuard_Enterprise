//! SPEC-005 AC-006 — top-level `process.name` strict normative + log-and-drop.
//!
//! Asserts the agent's defensive contract per ADR-0011 §5: when a captured
//! event has an empty `process.name` (i.e., `image_file_name` empty or basename
//! resolves to empty), the agent log-and-drops the event before envelope
//! construction. Two assertions:
//!   (1) the event MUST NOT enter the ring;
//!   (2) an error-level structured log line MUST identify the dropped event
//!       by PID and reason.
//!
//! Synthetic event injection at the ring enqueue boundary — the strict
//! normative defends against a future agent bug or ETW edge case, not against
//! an empirically-observed live condition (Phase 0 spike confirms Launch
//! events always carry `ImageFileName`).
//!
//! Pure Rust test — no testcontainers, no ETW, no ingest. Uses an inline
//! `MakeWriter` implementation to capture tracing output to an in-memory
//! buffer; if Phase 3.4.B+ ACs also need log capture, this pattern can be
//! extracted to `tests/common/log_capture.rs`.

use cg_agent::etw::{ActivityId, CapturedEvent, EventRing};
use std::io;
use std::sync::{Arc, Mutex};
use tracing::subscriber::with_default;
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone)]
struct BufferWriter {
    buf: Arc<Mutex<Vec<u8>>>,
}

impl io::Write for BufferWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.lock().unwrap().extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for BufferWriter {
    type Writer = BufferWriter;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[test]
fn ac_006_empty_process_name_logs_error_and_drops() {
    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let writer = BufferWriter {
        buf: Arc::clone(&buf),
    };

    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_max_level(tracing::Level::ERROR)
        .json()
        .finish();

    // Synthetic captured event with empty image_file_name → empty process.name.
    let event = CapturedEvent {
        pid: 12345,
        activity_id: ActivityId::Launch,
        image_file_name: String::new(), // empty — triggers log-and-drop
        parent_pid: 4,
        command_line: String::from("synthetic"),
        subject_user_sid: String::from("S-1-5-18"),
        etw_timestamp_nanos: 1716123612901000000,
        exit_status: None,
    };

    let ring = EventRing::new(1024);
    let pre_count = ring.len();

    with_default(subscriber, || {
        ring.enqueue_or_drop(event.clone());
    });

    // Assertion 1: the event was NOT enqueued — ring length unchanged.
    assert_eq!(
        ring.len(),
        pre_count,
        "AC-006: event with empty process.name MUST NOT enter the ring"
    );

    // Assertion 2: an error-level log line identifies the dropped event.
    let captured = buf.lock().unwrap();
    let log_output = String::from_utf8_lossy(&captured);
    assert!(
        log_output.contains("\"pid\":12345"),
        "AC-006: error log must include the dropped event's PID; got: {log_output}"
    );
    assert!(
        log_output.contains("image_file_name_empty"),
        "AC-006: error log must identify the drop reason; got: {log_output}"
    );
}
