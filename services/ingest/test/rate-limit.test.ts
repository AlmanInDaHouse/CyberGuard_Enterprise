import { expect, test } from "vitest";
import {
  ENROLL_FAILURE_LIMIT,
  ENROLL_TRACKED_MAX,
  ENROLL_WINDOW_MS,
  EnrollmentLimiter,
  RENEWAL_INTERVAL_MS,
  RenewalLimiter,
} from "../src/rate-limit.js";

// SPEC-025 §Operational §5 — the in-memory limits, under an injected clock: an address is
// refused until 15 minutes after its tenth failure in 15 minutes, failures older than the
// window do not count, one certificate per agent per 10 minutes, and the addresses tracked
// are bounded.

function clock(start = 1_000_000) {
  let t = start;
  return {
    now: () => t,
    advance: (ms: number) => {
      t += ms;
    },
  };
}

test("an address is refused from its tenth failure in 15 minutes until 15 minutes after it", () => {
  const c = clock();
  const limiter = new EnrollmentLimiter(c.now);
  for (let i = 0; i < ENROLL_FAILURE_LIMIT - 1; i++) {
    limiter.recordFailure("a");
    c.advance(60_000);
  }
  expect(limiter.refuses("a")).toBe(false);
  limiter.recordFailure("a");
  expect(limiter.refuses("a")).toBe(true);
  expect(limiter.refuses("b")).toBe(false);
  c.advance(ENROLL_WINDOW_MS - 1);
  expect(limiter.refuses("a")).toBe(true);
  c.advance(1);
  expect(limiter.refuses("a")).toBe(false);
});

test("failures older than 15 minutes do not count", () => {
  const c = clock();
  const limiter = new EnrollmentLimiter(c.now);
  for (let i = 0; i < ENROLL_FAILURE_LIMIT - 1; i++) limiter.recordFailure("a");
  c.advance(ENROLL_WINDOW_MS);
  limiter.recordFailure("a");
  expect(limiter.refuses("a")).toBe(false);
});

test("the addresses tracked are bounded", () => {
  const c = clock();
  const limiter = new EnrollmentLimiter(c.now);
  for (let i = 0; i < ENROLL_TRACKED_MAX + 50; i++) limiter.recordFailure(`addr-${i}`);
  expect(limiter.size).toBeLessThanOrEqual(ENROLL_TRACKED_MAX);
  // The latest addresses are the ones kept.
  for (let i = 0; i < ENROLL_FAILURE_LIMIT - 1; i++)
    limiter.recordFailure(`addr-${ENROLL_TRACKED_MAX + 49}`);
  expect(limiter.refuses(`addr-${ENROLL_TRACKED_MAX + 49}`)).toBe(true);
});

test("past 10000 addresses, those whose failures are older than 15 minutes are dropped", () => {
  const c = clock();
  const limiter = new EnrollmentLimiter(c.now);
  for (let i = 0; i < 10_001; i++) limiter.recordFailure(`old-${i}`);
  c.advance(ENROLL_WINDOW_MS);
  limiter.recordFailure("new");
  expect(limiter.size).toBe(1);
});

test("one certificate per agent per 10 minutes", () => {
  const c = clock();
  const limiter = new RenewalLimiter(c.now);
  expect(limiter.take("agent")).toBe(true);
  expect(limiter.take("other")).toBe(true);
  c.advance(RENEWAL_INTERVAL_MS - 1);
  expect(limiter.take("agent")).toBe(false);
  c.advance(1);
  expect(limiter.take("agent")).toBe(true);
});
