//! Heartbeat envelope types. See SPEC-001 §Data contracts.
//!
//! The envelope is a transport-level meta-message — not a CGES event.
//! It reuses the shape of `schemas/cges/v0.1/common/cg_agent.json`
//! for the `agent` sub-object.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Top-level heartbeat envelope, posted as JSON to
/// `{server.url}/v1/agents/heartbeat`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeartbeatEnvelope {
    pub envelope_version: String,
    pub agent: AgentBlock,
    pub sequence_number: u64,
    pub sent_at: String,
    pub status: HeartbeatStatus,
    pub uptime_seconds: u64,
}

/// Agent identity sub-object. Matches the four fields of
/// `schemas/cges/v0.1/common/cg_agent.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentBlock {
    pub agent_id: String,
    pub agent_version: String,
    pub agent_platform: String,
    pub agent_hostname: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HeartbeatStatus {
    Online,
    GoingOffline,
}

/// Schema version string carried by every envelope.
pub const ENVELOPE_VERSION: &str = "0.1.0";

/// Build a heartbeat envelope. `start_time` is the monotonic anchor
/// (Instant captured at process start, used for uptime); `now_wall`
/// is the UTC wall-clock that becomes `sent_at`.
pub fn build_envelope(
    agent: &AgentBlock,
    sequence_number: u64,
    start_time: Instant,
    now_wall: DateTime<Utc>,
    status: HeartbeatStatus,
) -> HeartbeatEnvelope {
    let uptime_seconds = start_time.elapsed().as_secs();
    HeartbeatEnvelope {
        envelope_version: ENVELOPE_VERSION.to_string(),
        agent: agent.clone(),
        sequence_number,
        sent_at: now_wall.to_rfc3339_opts(SecondsFormat::Millis, true),
        status,
        uptime_seconds,
    }
}
