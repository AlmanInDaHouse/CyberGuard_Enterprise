//! Shared helpers for the SPEC-001 and SPEC-002 integration harnesses.
//!
//! Per Cargo convention, `tests/common/mod.rs` is not treated as a
//! separate integration test; each test file includes it via
//! `mod common;`.

#![allow(dead_code)]

use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use base64::Engine as _;
use cg_agent::config::{
    load_from_path, AgentConfig, AgentIdentity, HeartbeatConfig, LogConfig, ServerConfig,
};
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::{NamedTempFile, TempDir};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// A valid self-signed Ed25519 X.509 certificate (PEM). Generated with
/// `openssl req -x509 -newkey ed25519` and `CN` set to a UUIDv7. Used as
/// the `client_certificate` the enroll mock returns, so the agent's
/// PEM-parse / DER-decode check (SPEC-002 §FR-006) sees a real cert.
pub const TEST_CLIENT_CERT_PEM: &str = "-----BEGIN CERTIFICATE-----\n\
MIIBczCCASWgAwIBAgIUNlfPyx/olnE258BmFTYeksUn77QwBQYDK2VwMC8xLTAr\n\
BgNVBAMMJDAxOTM0YWJjLWRlZjAtNzAwMC04OWFiLTAwMDAwMDAwMDA5OTAeFw0y\n\
NjA1MjEyMDU1NDhaFw0yNjA4MTkyMDU1NDhaMC8xLTArBgNVBAMMJDAxOTM0YWJj\n\
LWRlZjAtNzAwMC04OWFiLTAwMDAwMDAwMDA5OTAqMAUGAytlcAMhANrgsIrWMwsR\n\
kcYT4HYASqsfedvgJT1cuN4yO+6EWuVgo1MwUTAdBgNVHQ4EFgQUJREbBfjbOZLD\n\
OZrxh0N6VmkEOZwwHwYDVR0jBBgwFoAUJREbBfjbOZLDOZrxh0N6VmkEOZwwDwYD\n\
VR0TAQH/BAUwAwEB/zAFBgMrZXADQQAKUSegAHf/tARa/pLzs/dhkHaN/B2nnUht\n\
XuDkz+eLU50kmGJUM68Sim/2RYfgeWxjPTGlJfjFOQTf9HXw0SMP\n\
-----END CERTIFICATE-----\n";

/// The server-assigned `agent_id` (UUIDv7) the enroll mock returns. The
/// canonical identity from enrollment onward (SPEC-002 §Data contracts).
pub const TEST_AGENT_ID: &str = "01934abc-def0-7000-89ab-000000000099";

/// In-process axum mock for both SPEC-001 heartbeats and SPEC-002
/// enrollment.
///
/// - `POST /v1/agents/heartbeat` records every accepted (2xx) envelope;
///   callers may program a finite sequence of status codes.
/// - `POST /v1/agents/enroll` records every request body and returns a
///   canned `EnrollmentResponse` on 2xx (the default), or a programmed
///   error status. Useful for the token-rejected / server-5xx ACs.
pub struct MockServer {
    pub base_url: String,
    received: Arc<Mutex<Vec<Value>>>,
    response_plan: Arc<Mutex<Vec<u16>>>,
    enroll_received: Arc<Mutex<Vec<Value>>>,
    enroll_plan: Arc<Mutex<Vec<u16>>>,
    enroll_default_status: Arc<Mutex<u16>>,
    _task: JoinHandle<()>,
}

#[derive(Clone)]
struct AppState {
    received: Arc<Mutex<Vec<Value>>>,
    response_plan: Arc<Mutex<Vec<u16>>>,
    enroll_received: Arc<Mutex<Vec<Value>>>,
    enroll_plan: Arc<Mutex<Vec<u16>>>,
    enroll_default_status: Arc<Mutex<u16>>,
}

impl MockServer {
    pub async fn start() -> Self {
        let received: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
        let response_plan: Arc<Mutex<Vec<u16>>> = Arc::new(Mutex::new(Vec::new()));
        let enroll_received: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
        let enroll_plan: Arc<Mutex<Vec<u16>>> = Arc::new(Mutex::new(Vec::new()));
        let enroll_default_status: Arc<Mutex<u16>> = Arc::new(Mutex::new(200));

        let state = AppState {
            received: received.clone(),
            response_plan: response_plan.clone(),
            enroll_received: enroll_received.clone(),
            enroll_plan: enroll_plan.clone(),
            enroll_default_status: enroll_default_status.clone(),
        };

        let app = Router::new()
            .route("/v1/agents/heartbeat", post(receive_heartbeat))
            .route("/v1/agents/enroll", post(receive_enroll))
            .with_state(state);

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind mock server");
        let addr = listener.local_addr().expect("local_addr");
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        Self {
            base_url: format!("http://{addr}"),
            received,
            response_plan,
            enroll_received,
            enroll_plan,
            enroll_default_status,
            _task: task,
        }
    }

    // --- Heartbeat introspection (SPEC-001) ---

    pub fn received_count(&self) -> usize {
        self.received.lock().expect("received lock").len()
    }

    pub fn received(&self) -> Vec<Value> {
        self.received.lock().expect("received lock").clone()
    }

    /// Program the next N heartbeat HTTP responses. Empty plan ⇒ 200.
    pub fn set_response_plan(&self, codes: Vec<u16>) {
        let mut plan = self.response_plan.lock().expect("response_plan lock");
        plan.clear();
        plan.extend(codes);
    }

    // --- Enrollment introspection (SPEC-002) ---

    pub fn enroll_received_count(&self) -> usize {
        self.enroll_received.lock().expect("enroll lock").len()
    }

    pub fn enroll_received(&self) -> Vec<Value> {
        self.enroll_received.lock().expect("enroll lock").clone()
    }

    /// Forget all recorded enrollment requests (used to assert a second
    /// run does not POST to `/v1/agents/enroll`).
    pub fn reset_enroll(&self) {
        self.enroll_received.lock().expect("enroll lock").clear();
    }

    /// Program a finite sequence of enrollment HTTP statuses (consumed
    /// one per request). After the sequence is exhausted, the default
    /// status applies.
    pub fn set_enroll_plan(&self, codes: Vec<u16>) {
        let mut plan = self.enroll_plan.lock().expect("enroll_plan lock");
        plan.clear();
        plan.extend(codes);
    }

    /// Set the status returned for every enrollment request once the
    /// finite plan is empty. Defaults to 200. Use 500 to make every
    /// retry fail (AC-008).
    pub fn set_enroll_default(&self, code: u16) {
        *self
            .enroll_default_status
            .lock()
            .expect("enroll_default lock") = code;
    }
}

async fn receive_heartbeat(State(s): State<AppState>, Json(envelope): Json<Value>) -> StatusCode {
    let next_code = {
        let mut plan = s.response_plan.lock().expect("response_plan lock");
        if plan.is_empty() {
            200
        } else {
            plan.remove(0)
        }
    };
    // Only record envelopes accepted with 2xx, so AC-005/AC-006/AC-010 can
    // distinguish "received and accepted" from "received and rejected".
    if (200..300).contains(&next_code) {
        s.received.lock().expect("received lock").push(envelope);
    }
    StatusCode::from_u16(next_code).unwrap_or(StatusCode::OK)
}

async fn receive_enroll(
    State(s): State<AppState>,
    Json(req): Json<Value>,
) -> (StatusCode, Json<Value>) {
    // Record every enrollment attempt regardless of the status returned,
    // so AC-003 can inspect the body and AC-005 can assert zero attempts.
    s.enroll_received.lock().expect("enroll lock").push(req);

    let code = {
        let mut plan = s.enroll_plan.lock().expect("enroll_plan lock");
        if plan.is_empty() {
            *s.enroll_default_status.lock().expect("enroll_default lock")
        } else {
            plan.remove(0)
        }
    };
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::OK);

    if (200..300).contains(&code) {
        (status, Json(canned_enroll_response()))
    } else {
        (
            status,
            Json(json!({ "error": "enrollment rejected by mock" })),
        )
    }
}

/// The success body for `POST /v1/agents/enroll` (SPEC-002 §Data
/// contracts): a fresh `agent_id`, a real PEM cert, and ISO 8601 stamps.
fn canned_enroll_response() -> Value {
    json!({
        "envelope_version": "0.1.0",
        "agent_id": TEST_AGENT_ID,
        "client_certificate": TEST_CLIENT_CERT_PEM,
        "issued_at": "2026-05-21T10:23:11.482Z",
        "expires_at": "2026-08-19T10:23:11.482Z"
    })
}

/// Write `body` to a temporary TOML file and return the handle. The
/// caller must keep the handle alive for the duration of the test.
pub fn write_temp_config(body: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().expect("tempfile");
    f.write_all(body.as_bytes()).expect("write");
    f.flush().expect("flush");
    f
}

/// Build a minimal valid AgentConfig pointing at the given mock URL,
/// with the heartbeat interval overridden to `interval_seconds`. Used
/// to bypass on-disk config for tests that exercise `run()`.
pub fn config_with_url(server_url: &str, interval_seconds: u64) -> AgentConfig {
    AgentConfig {
        server: ServerConfig {
            url: server_url.to_string(),
            trust_anchor_path: None,
        },
        agent: AgentIdentity {
            id: "01934abc-def0-7000-89ab-000000000001".to_string(),
            hostname: "FIN-PC-014".to_string(),
        },
        heartbeat: HeartbeatConfig {
            interval_seconds,
            request_timeout_seconds: 5,
            max_retries: 3,
            backoff_initial_ms: 50,
            backoff_factor: 2.0,
            backoff_max_ms: 1000,
        },
        log: LogConfig {
            level: "info".to_string(),
        },
        enrollment: None,
        tls: None,
        envelope: None,
    }
}

/// A first-run enrollment fixture: a temp dir holding a real `agent.toml`
/// (with the `[enrollment]` token + artifact paths) and the not-yet-created
/// `cert.pem` / `key.dat` / `identity.json` paths. The `AgentConfig` is
/// loaded through the real parser so the SPEC-002 `[enrollment]` serde
/// wiring is exercised end to end. Keep the fixture alive for the whole
/// test — dropping it deletes the temp dir.
pub struct EnrollmentFixture {
    pub config: AgentConfig,
    pub config_path: PathBuf,
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
    pub identity_path: PathBuf,
    _dir: TempDir,
}

impl EnrollmentFixture {
    /// True iff all three persisted identity artifacts exist on disk.
    pub fn all_artifacts_exist(&self) -> bool {
        self.cert_path.exists() && self.key_path.exists() && self.identity_path.exists()
    }

    /// True iff none of the three persisted artifacts exist (the
    /// expected state after a failed enrollment — FR-012 / NFR-005).
    pub fn no_artifacts_exist(&self) -> bool {
        !self.cert_path.exists() && !self.key_path.exists() && !self.identity_path.exists()
    }
}

/// Build an [`EnrollmentFixture`] whose `agent.toml` points enrollment at
/// `server_url` and carries `token`. Backoff is set fast (10 ms initial)
/// so the retry-exhaustion ACs (AC-008 / AC-009) run quickly.
pub fn enrollment_fixture(server_url: &str, token: &str) -> EnrollmentFixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let cert_path = dir.path().join("cert.pem");
    let key_path = dir.path().join("key.dat");
    let identity_path = dir.path().join("identity.json");
    let config_path = dir.path().join("agent.toml");

    // Paths go in TOML *literal* strings (single quotes) so Windows
    // backslashes are not treated as escape sequences.
    let toml = format!(
        "[server]\n\
         url = \"{server_url}\"\n\
         \n\
         [agent]\n\
         id = \"01934abc-def0-7000-89ab-000000000001\"\n\
         hostname = \"FIN-PC-014\"\n\
         \n\
         [heartbeat]\n\
         interval_seconds = 1\n\
         request_timeout_seconds = 5\n\
         max_retries = 3\n\
         backoff_initial_ms = 50\n\
         backoff_factor = 2.0\n\
         backoff_max_ms = 1000\n\
         \n\
         [enrollment]\n\
         token = \"{token}\"\n\
         cert_path = '{cert}'\n\
         key_path = '{key}'\n\
         identity_path = '{identity}'\n\
         timeout_seconds = 30\n\
         max_retries = 3\n\
         backoff_initial_ms = 10\n\
         backoff_factor = 2.0\n",
        server_url = server_url,
        token = token,
        cert = cert_path.display(),
        key = key_path.display(),
        identity = identity_path.display(),
    );
    std::fs::write(&config_path, toml).expect("write agent.toml");

    let config = load_from_path(&config_path).expect("enrollment fixture config must be valid");

    EnrollmentFixture {
        config,
        config_path,
        cert_path,
        key_path,
        identity_path,
        _dir: dir,
    }
}

/// A `base_url` whose port is closed: a listener is bound to obtain a
/// free port, then dropped, so connection attempts are refused. Drives
/// the network-unreachable AC (AC-009).
pub async fn closed_port_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind for closed port");
    let addr = listener.local_addr().expect("local_addr");
    drop(listener);
    format!("http://{addr}")
}

/// Decode a base64url-unpadded string (the `agent_pubkey` wire encoding,
/// SPEC-002 §Data contracts) back to raw bytes.
pub fn base64url_decode(s: &str) -> Vec<u8> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .expect("valid base64url-unpadded")
}
