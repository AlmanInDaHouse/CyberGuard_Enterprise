//! Shared helpers for the SPEC-001 and SPEC-002 integration harnesses.
//!
//! Per Cargo convention, `tests/common/mod.rs` is not treated as a
//! separate integration test; each test file includes it via
//! `mod common;`.

#![allow(dead_code)]

use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use base64::Engine as _;
use cg_agent::config::{
    load_from_path, AgentConfig, AgentIdentity, EnvelopeConfig, HeartbeatConfig, LogConfig,
    ServerConfig, TlsConfig,
};
use ed25519_dalek::pkcs8::DecodePrivateKey;
use rcgen::{CertificateParams, DnType, KeyPair, SanType, PKCS_ED25519};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::{NamedTempFile, TempDir};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;

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
            heartbeat_url: None,
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

// =====================================================================
// SPEC-003 TLS harness (test-only). Trust material is generated fresh at
// setup with rcgen and never committed.
// =====================================================================

/// Test PKI: a CA the agent trusts for the server, a CA-signed server
/// cert (good) and a rogue-CA-signed one (for the untrusted-server case),
/// and an Ed25519 client cert (the agent identity) signed by the trusted
/// CA. `agent_seed` / `agent_pubkey` are the raw key bytes behind the
/// client cert, so the agent can sign and the mock can verify.
pub struct TlsTestPki {
    pub agent_id: String,
    pub trust_anchor_pem: String,
    pub agent_seed: [u8; 32],
    pub agent_pubkey: [u8; 32],
    pub client_cert_pem: String,
    good_server_cert: rustls::pki_types::CertificateDer<'static>,
    good_server_key_der: Vec<u8>,
    rogue_server_cert: rustls::pki_types::CertificateDer<'static>,
    rogue_server_key_der: Vec<u8>,
    trusted_client_root: rustls::pki_types::CertificateDer<'static>,
    rogue_client_root: rustls::pki_types::CertificateDer<'static>,
}

fn make_ca(cn: &str) -> (rcgen::Certificate, KeyPair) {
    let key = KeyPair::generate().expect("ca key");
    let mut params = CertificateParams::new(Vec::<String>::new()).expect("ca params");
    params.distinguished_name.push(DnType::CommonName, cn);
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    let cert = params.self_signed(&key).expect("self-signed ca");
    (cert, key)
}

fn make_server_cert(ca: &rcgen::Certificate, ca_key: &KeyPair) -> (rcgen::Certificate, KeyPair) {
    let key = KeyPair::generate().expect("server key");
    let mut params = CertificateParams::new(vec!["localhost".to_string()]).expect("server params");
    params
        .subject_alt_names
        .push(SanType::IpAddress(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
    params
        .distinguished_name
        .push(DnType::CommonName, "localhost");
    let cert = params
        .signed_by(&key, ca, ca_key)
        .expect("ca-signed server cert");
    (cert, key)
}

/// Generate a fresh test PKI bound to `agent_id`.
pub fn generate_test_pki(agent_id: &str) -> TlsTestPki {
    let (ca, ca_key) = make_ca("CyberGuard Test CA");
    let (server, server_key) = make_server_cert(&ca, &ca_key);
    let (rogue_ca, rogue_ca_key) = make_ca("Rogue CA");
    let (rogue_server, rogue_server_key) = make_server_cert(&rogue_ca, &rogue_ca_key);

    let client_key = KeyPair::generate_for(&PKCS_ED25519).expect("client ed25519 key");
    let mut client_params = CertificateParams::new(Vec::<String>::new()).expect("client params");
    client_params
        .distinguished_name
        .push(DnType::CommonName, agent_id);
    let client_cert = client_params
        .signed_by(&client_key, &ca, &ca_key)
        .expect("ca-signed client cert");

    let signing = ed25519_dalek::SigningKey::from_pkcs8_der(&client_key.serialize_der())
        .expect("ed25519 from pkcs8");

    TlsTestPki {
        agent_id: agent_id.to_string(),
        trust_anchor_pem: ca.pem(),
        agent_seed: signing.to_bytes(),
        agent_pubkey: signing.verifying_key().to_bytes(),
        client_cert_pem: client_cert.pem(),
        good_server_cert: server.der().clone(),
        good_server_key_der: server_key.serialize_der(),
        rogue_server_cert: rogue_server.der().clone(),
        rogue_server_key_der: rogue_server_key.serialize_der(),
        trusted_client_root: ca.der().clone(),
        rogue_client_root: rogue_ca.der().clone(),
    }
}

/// Adversarial knob for the TLS mock.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TlsMockMode {
    /// Valid server cert chaining to the trust anchor; accepts the client.
    Normal,
    /// Server presents a cert NOT chaining to the agent's trust anchor.
    UntrustedServerCert,
    /// Client verifier uses a root the agent's cert does not chain to.
    RejectClientCert,
    /// Server offers only TLS 1.2.
    Tls12Only,
}

/// In-process TLS-terminating mock for the secure heartbeat path. Records
/// every accepted outer envelope. Verifies the Ed25519 signature over the
/// JCS(outer-minus-signature), rejects replayed nonces, and (optionally)
/// rejects every envelope to exercise the agent's warn-and-continue path.
pub struct TlsMockServer {
    pub base_url: String,
    received: Arc<Mutex<Vec<Value>>>,
    _task: JoinHandle<()>,
}

struct TlsMockState {
    received: Arc<Mutex<Vec<Value>>>,
    seen_nonces: Mutex<HashSet<String>>,
    agent_pubkey: [u8; 32],
    /// When set, every envelope is answered with this status (simulates a
    /// server-side rejection, e.g. timestamp skew — AC-005).
    reject_status: Option<u16>,
}

fn ring_provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn build_server_config(pki: &TlsTestPki, mode: TlsMockMode) -> rustls::ServerConfig {
    let (cert, key_der) = match mode {
        TlsMockMode::UntrustedServerCert => (
            pki.rogue_server_cert.clone(),
            pki.rogue_server_key_der.clone(),
        ),
        _ => (
            pki.good_server_cert.clone(),
            pki.good_server_key_der.clone(),
        ),
    };
    let client_root = match mode {
        TlsMockMode::RejectClientCert => pki.rogue_client_root.clone(),
        _ => pki.trusted_client_root.clone(),
    };

    let mut roots = rustls::RootCertStore::empty();
    roots.add(client_root).expect("add client root");
    let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
        Arc::new(roots),
        ring_provider(),
    )
    .build()
    .expect("client verifier");

    let v13 = [&rustls::version::TLS13];
    let v12 = [&rustls::version::TLS12];
    let versions: &[&rustls::SupportedProtocolVersion] = match mode {
        TlsMockMode::Tls12Only => &v12,
        _ => &v13,
    };

    let key = rustls::pki_types::PrivateKeyDer::try_from(key_der).expect("server key der");
    rustls::ServerConfig::builder_with_provider(ring_provider())
        .with_protocol_versions(versions)
        .expect("protocol versions")
        .with_client_cert_verifier(verifier)
        .with_single_cert(vec![cert], key)
        .expect("server cert")
}

impl TlsMockServer {
    /// Start a TLS mock in the given `mode`, accepting valid signed
    /// envelopes.
    pub async fn start(pki: &TlsTestPki, mode: TlsMockMode) -> Self {
        Self::start_inner(pki, mode, None).await
    }

    /// Start a TLS mock that completes the handshake but answers every
    /// signed envelope with `reject_status` (simulates a server-side
    /// rejection such as timestamp skew — AC-005). TLS itself is Normal.
    pub async fn start_rejecting(pki: &TlsTestPki, reject_status: u16) -> Self {
        Self::start_inner(pki, TlsMockMode::Normal, Some(reject_status)).await
    }

    async fn start_inner(pki: &TlsTestPki, mode: TlsMockMode, reject_status: Option<u16>) -> Self {
        let received: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
        let acceptor = TlsAcceptor::from(Arc::new(build_server_config(pki, mode)));
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind tls mock");
        let port = listener.local_addr().expect("local_addr").port();

        let state = Arc::new(TlsMockState {
            received: received.clone(),
            seen_nonces: Mutex::new(HashSet::new()),
            agent_pubkey: pki.agent_pubkey,
            reject_status,
        });

        let task = tokio::spawn(async move {
            loop {
                let (tcp, _) = match listener.accept().await {
                    Ok(v) => v,
                    Err(_) => break,
                };
                let acceptor = acceptor.clone();
                let state = state.clone();
                tokio::spawn(async move {
                    if let Ok(tls) = acceptor.accept(tcp).await {
                        let _ = handle_tls_conn(tls, state).await;
                    }
                });
            }
        });

        Self {
            base_url: format!("https://localhost:{port}"),
            received,
            _task: task,
        }
    }

    pub fn received(&self) -> Vec<Value> {
        self.received.lock().expect("received lock").clone()
    }

    pub fn received_count(&self) -> usize {
        self.received.lock().expect("received lock").len()
    }
}

async fn handle_tls_conn(
    mut tls: tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
    state: Arc<TlsMockState>,
) -> std::io::Result<()> {
    let mut buf = [0u8; 8192];
    let mut data: Vec<u8> = Vec::new();

    loop {
        let n = tls.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);

        if let Some(hdr_end) = find_subsequence(&data, b"\r\n\r\n") {
            let content_length = parse_content_length(&data[..hdr_end]);
            let body_start = hdr_end + 4;
            if data.len() >= body_start + content_length {
                let status = process_body(&data[body_start..body_start + content_length], &state);
                let resp = format!(
                    "HTTP/1.1 {status} {}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                    status_text(status)
                );
                tls.write_all(resp.as_bytes()).await?;
                let _ = tls.shutdown().await;
                break;
            }
        }
    }
    Ok(())
}

fn process_body(body: &[u8], state: &TlsMockState) -> u16 {
    let envelope: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return 400,
    };
    if let Some(status) = state.reject_status {
        return status;
    }

    // Verify the Ed25519 signature over JCS(outer minus signature).
    let sig_b64 = match envelope.get("signature").and_then(Value::as_str) {
        Some(s) => s,
        None => return 400,
    };
    let sig_bytes = base64url_decode(sig_b64);
    let sig_arr: [u8; 64] = match sig_bytes.try_into() {
        Ok(a) => a,
        Err(_) => return 400,
    };
    let mut signed = envelope.clone();
    if let Some(obj) = signed.as_object_mut() {
        obj.remove("signature");
    }
    let canonical = match serde_jcs::to_vec(&signed) {
        Ok(c) => c,
        Err(_) => return 400,
    };
    let verifying_key = match ed25519_dalek::VerifyingKey::from_bytes(&state.agent_pubkey) {
        Ok(k) => k,
        Err(_) => return 400,
    };
    let signature = ed25519_dalek::Signature::from_bytes(&sig_arr);
    if verifying_key.verify_strict(&canonical, &signature).is_err() {
        return 401;
    }

    // Replay defense: reject a nonce we have already seen.
    let nonce = envelope
        .get("nonce")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    {
        let mut seen = state.seen_nonces.lock().expect("nonce lock");
        if !seen.insert(nonce) {
            return 409;
        }
    }

    state.received.lock().expect("received lock").push(envelope);
    200
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn parse_content_length(headers: &[u8]) -> usize {
    let text = String::from_utf8_lossy(headers).to_lowercase();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("content-length:") {
            return rest.trim().parse().unwrap_or(0);
        }
    }
    0
}

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        409 => "Conflict",
        _ => "Error",
    }
}

/// A secure-path fixture: an `AgentConfig` whose `trust_anchor_path`
/// points at a temp PEM of the test CA (activating the SPEC-003 secure
/// path), plus the matching agent `Identity`. Keep it alive for the test
/// (its temp file backs `trust_anchor_path`).
pub struct SecureFixture {
    pub config: AgentConfig,
    pub identity: cg_agent::identity::Identity,
    /// Backs `config.server.trust_anchor_path`; keep alive for the test.
    pub trust_anchor: NamedTempFile,
}

/// Build a [`SecureFixture`] pointing the secure path at `server_url`,
/// trusting `pki`'s CA, and carrying `pki`'s agent identity.
pub fn secure_fixture(server_url: &str, pki: &TlsTestPki) -> SecureFixture {
    let mut trust_anchor = NamedTempFile::new().expect("trust anchor tempfile");
    trust_anchor
        .write_all(pki.trust_anchor_pem.as_bytes())
        .expect("write trust anchor");
    trust_anchor.flush().expect("flush trust anchor");

    let config = AgentConfig {
        server: ServerConfig {
            url: server_url.to_string(),
            trust_anchor_path: Some(trust_anchor.path().display().to_string()),
            heartbeat_url: None,
        },
        agent: AgentIdentity {
            id: pki.agent_id.clone(),
            hostname: "FIN-PC-014".to_string(),
        },
        heartbeat: HeartbeatConfig {
            interval_seconds: 1,
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
        tls: Some(TlsConfig {
            minimum_version: "1.3".to_string(),
        }),
        envelope: Some(EnvelopeConfig {
            canonical_form: "JCS".to_string(),
        }),
    };

    let identity = cg_agent::identity::Identity {
        agent_id: pki.agent_id.clone(),
        keypair: cg_agent::crypto::AgentKeypair::from_secret_bytes(&pki.agent_seed),
        client_certificate_pem: pki.client_cert_pem.clone(),
    };

    SecureFixture {
        config,
        identity,
        trust_anchor,
    }
}
