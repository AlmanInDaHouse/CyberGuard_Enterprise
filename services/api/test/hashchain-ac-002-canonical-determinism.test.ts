import { expect, test } from "vitest";
import { canonicalEvent } from "../src/forensic/canonical.js";
import { evidenceHash, toHex } from "../src/forensic/hashchain.js";
import type { TimelineEvent } from "../src/read/types.js";

// SPEC-012 hashchain_ac_002 (SC-HC-002) — canonicalization is byte-for-byte
// deterministic: the JCS (RFC 8785) form of a TimelineEvent is identical across
// runs and INDEPENDENT of the insertion order of the object's fields. Pure (no
// backend); the api global-setup still starts the shared containers but this test
// touches none of them.

const base: TimelineEvent = {
  event_id: "01934abc-def0-7000-89ab-000000000001",
  agent_id: "01934abc-def0-7000-89ab-0000000000a1",
  activity_id: 1,
  process_pid: 4321,
  process_uid: "S-1-5-21-1004336348",
  process_name: "powershell.exe",
  image_file_name: "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
  process_parent_pid: 1000,
  event_time: "2026-06-05 10:00:00.000000000",
};

test("hashchain_ac_002: canonical form is byte-stable across runs", () => {
  expect(canonicalEvent(base)).toBe(canonicalEvent(base));
});

test("hashchain_ac_002: canonical form is independent of field insertion order", () => {
  // Same data, fields inserted in a DIFFERENT order — JCS must sort the keys.
  const reordered: TimelineEvent = {
    event_time: base.event_time,
    process_parent_pid: base.process_parent_pid,
    image_file_name: base.image_file_name,
    process_name: base.process_name,
    process_uid: base.process_uid,
    process_pid: base.process_pid,
    activity_id: base.activity_id,
    agent_id: base.agent_id,
    event_id: base.event_id,
  };
  expect(canonicalEvent(reordered)).toBe(canonicalEvent(base));
});

test("hashchain_ac_002: a null parent_pid canonicalizes deterministically", () => {
  const withNull: TimelineEvent = { ...base, process_parent_pid: null };
  expect(canonicalEvent(withNull)).toBe(canonicalEvent({ ...base, process_parent_pid: null }));
  // and differs from the non-null event.
  expect(canonicalEvent(withNull)).not.toBe(canonicalEvent(base));
});

test("hashchain_ac_002: identical events hash identically (evidence_n.hash stable)", async () => {
  expect(toHex(await evidenceHash(base))).toBe(toHex(await evidenceHash(base)));
});
