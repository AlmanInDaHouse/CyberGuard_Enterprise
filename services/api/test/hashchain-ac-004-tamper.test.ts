import { expect, test } from "vitest";
import { computeChainHex, verifyChain } from "../src/forensic/hashchain.js";
import type { TimelineEvent } from "../src/read/types.js";

// SPEC-012 hashchain_ac_004 (SC-HC-004) — tamper-evidence: mutating any byte of any
// evidence unit (or reordering, or dropping/adding an event) changes chain_N, and
// verifyChain against the original chain fails. Pure (no backend). Also pins the
// genesis chain_0 = SHA-256("").

const events: TimelineEvent[] = [
  {
    event_id: "01934abc-def0-7000-89ab-000000000001",
    agent_id: "01934abc-def0-7000-89ab-0000000000a1",
    activity_id: 1,
    process_pid: 4321,
    process_uid: "S-1-5-21-1004336348",
    process_name: "winword.exe",
    image_file_name: "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe",
    process_parent_pid: 1000,
    event_time: "2026-06-05 10:00:00.000000000",
  },
  {
    event_id: "01934abc-def0-7000-89ab-000000000002",
    agent_id: "01934abc-def0-7000-89ab-0000000000a1",
    activity_id: 1,
    process_pid: 4322,
    process_uid: "S-1-5-21-1004336348",
    process_name: "powershell.exe",
    image_file_name: "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
    process_parent_pid: 4321,
    event_time: "2026-06-05 10:00:01.000000000",
  },
];

test("hashchain_ac_004: a single-byte mutation changes chain_N and fails verification", async () => {
  const original = await computeChainHex(events);
  expect(await verifyChain(events, original)).toBe(true);

  // Mutate ONE byte: one character of one event's process_name.
  const tampered = events.map((e, i) => (i === 1 ? { ...e, process_name: "powersgell.exe" } : e));
  const tamperedChain = await computeChainHex(tampered);
  expect(tamperedChain).not.toBe(original);
  expect(await verifyChain(tampered, original)).toBe(false);
});

test("hashchain_ac_004: a numeric mutation (process_pid) changes chain_N", async () => {
  const original = await computeChainHex(events);
  const tampered = events.map((e, i) => (i === 0 ? { ...e, process_pid: e.process_pid + 1 } : e));
  expect(await computeChainHex(tampered)).not.toBe(original);
});

test("hashchain_ac_004: reordering events changes chain_N (the order is bound in)", async () => {
  const original = await computeChainHex(events);
  expect(await computeChainHex([...events].reverse())).not.toBe(original);
});

test("hashchain_ac_004: dropping an event changes chain_N", async () => {
  const original = await computeChainHex(events);
  expect(await computeChainHex([events[0] as TimelineEvent])).not.toBe(original);
});

test('hashchain_ac_004: empty evidence yields the genesis chain_0 = SHA-256("")', async () => {
  // The well-known SHA-256 of the empty input.
  expect(await computeChainHex([])).toBe(
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  );
});
