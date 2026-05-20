//! Shared helpers for the SPEC-001 integration harness.
//!
//! Per Cargo convention, `tests/common/mod.rs` is not treated as a
//! separate integration test; each test file includes it via
//! `mod common;`.

#![allow(dead_code)]

use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use cg_agent::config::{AgentConfig, AgentIdentity, HeartbeatConfig, LogConfig, ServerConfig};
use serde_json::Value;
use std::io::Write;
use std::sync::{Arc, Mutex};
use tempfile::NamedTempFile;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// In-process axum mock that records every heartbeat envelope received
/// at `POST /v1/agents/heartbeat`. Returns 200 by default; callers may
/// program a finite sequence of status codes for the next N requests.
pub struct MockServer {
    pub base_url: String,
    received: Arc<Mutex<Vec<Value>>>,
    response_plan: Arc<Mutex<Vec<u16>>>,
    _task: JoinHandle<()>,
}

#[derive(Clone)]
struct AppState {
    received: Arc<Mutex<Vec<Value>>>,
    response_plan: Arc<Mutex<Vec<u16>>>,
}

impl MockServer {
    pub async fn start() -> Self {
        let received: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
        let response_plan: Arc<Mutex<Vec<u16>>> = Arc::new(Mutex::new(Vec::new()));

        let state = AppState {
            received: received.clone(),
            response_plan: response_plan.clone(),
        };

        let app = Router::new()
            .route("/v1/agents/heartbeat", post(receive_heartbeat))
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
            _task: task,
        }
    }

    pub fn received_count(&self) -> usize {
        self.received.lock().expect("received lock").len()
    }

    pub fn received(&self) -> Vec<Value> {
        self.received.lock().expect("received lock").clone()
    }

    /// Program the next N HTTP responses. Empty plan ⇒ always 200.
    pub fn set_response_plan(&self, codes: Vec<u16>) {
        let mut plan = self.response_plan.lock().expect("response_plan lock");
        plan.clear();
        plan.extend(codes);
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
    }
}
