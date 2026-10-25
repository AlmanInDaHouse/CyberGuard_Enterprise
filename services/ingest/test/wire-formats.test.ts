import { expect, test } from "vitest";
import { IpTextSchema, OuterEnvelopeSchema } from "../src/schemas.js";

// The wire's formats as zod 3.25.76 read them, kept across the upgrade to zod 4, whose
// z.uuid(), .int() and IP formats read them otherwise. Each expected value is what zod 3
// answered for the input, recorded when the upgrade was made. Each format is read where
// the heartbeat carries it: the event ids of the three kinds of event, the addresses of a
// connection's two endpoints and of a logon's source, the outer and the inner sequence
// numbers and the uptime.

/** A well-formed outer envelope, with the numbers and the events given. */
function envelope(over: {
  outer_sequence_number?: number;
  sequence_number?: number;
  uptime_seconds?: number;
  events?: unknown[];
}) {
  return {
    outer_envelope_version: "1",
    agent_id: "019a1f3e-7c2d-7b44-9e1a-5d6c7b8a9f02",
    sequence_number: over.outer_sequence_number ?? 1,
    nonce: "n",
    sent_at: "2026-10-25T00:00:00Z",
    body: {
      envelope_version: "1",
      agent: {
        agent_id: "019a1f3e-7c2d-7b44-9e1a-5d6c7b8a9f02",
        agent_version: "0.1.0",
        agent_platform: "windows",
        agent_hostname: "HOST",
      },
      sequence_number: over.sequence_number ?? 1,
      sent_at: "2026-10-25T00:00:00Z",
      status: "online",
      uptime_seconds: over.uptime_seconds ?? 1,
      events: over.events ?? [],
    },
    signature: "s",
  };
}

/** A Process Activity event (SPEC-005). */
function processEvent(eventId: string) {
  return {
    event_id: eventId,
    class_uid: 1007,
    activity_id: 1,
    time: "1700000000000000000",
    process: {
      pid: 1,
      uid: "u",
      name: "n",
      created_time: null,
      parent_pid: null,
      command_line: "",
      subject_user_sid: "",
      image_file_name: "",
    },
  };
}

/** A Network Activity event (SPEC-019), its source and destination addresses given. */
function networkEvent(eventId: string, src = "192.0.2.10", dst = "198.51.100.7") {
  return {
    event_id: eventId,
    class_uid: 4001,
    activity_id: 1,
    time: "1700000000000002000",
    src_endpoint: { ip: src, port: 49213 },
    dst_endpoint: { ip: dst, port: 443 },
    connection_info: { protocol_name: "tcp", direction: "outbound" },
    actor: { process: { pid: 4321 } },
  };
}

/** A refused logon (SPEC-020), its source address given. */
function logonEvent(eventId: string, ip = "192.0.2.10") {
  return {
    event_id: eventId,
    class_uid: 3002,
    category_uid: 3,
    activity_id: 1,
    time: "1700000000000003000",
    status_id: 2,
    user: { uid: "S-1-0-0", name: "<withheld>", domain: "<withheld>" },
    logon_type_id: 3,
    auth_protocol: "NTLM",
    auth_protocol_id: 1,
    src_endpoint: { ip, hostname: "WS-0042" },
    status_code: "0xc000006d",
    status_detail: "0xc0000064",
  };
}

const VALID_ID = "019a1f3e-7c2d-7b44-9e1a-5d6c7b8a9f01";
const accepts = (events: unknown[]) => OuterEnvelopeSchema.safeParse(envelope({ events })).success;

test("the reference envelope, with one event of each kind, is accepted", () => {
  const parsed = OuterEnvelopeSchema.safeParse(
    envelope({ events: [processEvent(VALID_ID), networkEvent(VALID_ID), logonEvent(VALID_ID)] }),
  );
  expect(parsed.success, JSON.stringify(parsed.error?.issues)).toBe(true);
});

const IP_TEXT: [string, boolean][] = [
  ["127.0.0.1", true],
  ["0.0.0.0", true],
  ["255.255.255.255", true],
  ["256.1.1.1", false],
  ["01.2.3.4", false],
  ["1.2.3", false],
  ["1.2.3.4.5", false],
  ["1a2b3c4", false],
  ["::1", true],
  ["::", true],
  ["fe80::1", true],
  ["fe80::1%eth0", true],
  ["fe80::1%25eth0", true],
  ["::ffff:10.0.0.5", true],
  ["2001:db8::1", true],
  ["2001:DB8::1", true],
  ["1:2:3:4:5:6:7:8", true],
  ["1:2:3:4:5:6:7:8:9", false],
  ["1:2:3:4:5:6:1.2.3.4", false],
  ["2001:db8::1%eth0", false],
  ["1::2::3", false],
  ["1.2.3.4/24", false],
  [" 1.2.3.4", false],
  ["", false],
];

test.each(IP_TEXT)("an IP address in text: %j is accepted: %s", (input, accepted) => {
  expect(IpTextSchema.safeParse(input).success).toBe(accepted);
});

test.each(IP_TEXT)(
  "a connection's source, its destination and a logon's source of %j are accepted: %s",
  (input, accepted) => {
    expect(accepts([networkEvent(VALID_ID, input, "198.51.100.7")])).toBe(accepted);
    expect(accepts([networkEvent(VALID_ID, "192.0.2.10", input)])).toBe(accepted);
    expect(accepts([logonEvent(VALID_ID, input)])).toBe(accepted);
  },
);

test.each<[string, boolean]>([
  [VALID_ID, true],
  ["019A1F3E-7C2D-7B44-9E1A-5D6C7B8A9F01", true],
  ["00000000-0000-0000-0000-000000000000", true],
  ["21436587-2143-1143-2143-214365872143", true],
  ["5f0e2d1c-3b4a-9c8d-a7b6-0123456789ab", true],
  ["{019a1f3e-7c2d-7b44-9e1a-5d6c7b8a9f01}", false],
  ["019a1f3e7c2d7b449e1a5d6c7b8a9f01", false],
  ["", false],
])("an event_id %j is accepted on a process, a connection and a logon: %s", (input, accepted) => {
  expect(accepts([processEvent(input)])).toBe(accepted);
  expect(accepts([networkEvent(input)])).toBe(accepted);
  expect(accepts([logonEvent(input)])).toBe(accepted);
});

test.each<[number, boolean]>([
  [0, true],
  [1, true],
  [2 ** 53, true],
  [2 ** 60, true],
  [-1, false],
  [1.5, false],
])("an outer or inner sequence number, or an uptime, of %d is accepted: %s", (n, accepted) => {
  expect(OuterEnvelopeSchema.safeParse(envelope({ outer_sequence_number: n })).success).toBe(
    accepted,
  );
  expect(OuterEnvelopeSchema.safeParse(envelope({ sequence_number: n })).success).toBe(accepted);
  expect(OuterEnvelopeSchema.safeParse(envelope({ uptime_seconds: n })).success).toBe(accepted);
});
