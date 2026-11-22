import type { Pool } from "pg";
import { BadRequestError } from "../errors.js";
import { fromStore } from "../stores.js";

/**
 * SPEC-026 §Data contracts §3, *Users and invitations* — the users' two lists, each a page of
 * `limit` (default 50, at most 200) after an opaque `cursor`, keyset over the order it names,
 * answered as `{ items, next_cursor }` (SPEC-009 NFR-009-001). A cursor the api did not issue is
 * a 400 before any store is read.
 */

const MAX_LIMIT = 200;
const DEFAULT_LIMIT = 50;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export interface PageParams {
  limit?: unknown;
  cursor?: unknown;
}

export interface Page<T> {
  items: T[];
  next_cursor: string | null;
}

function clampLimit(raw: unknown): number {
  const n = Number(raw);
  return Number.isFinite(n) && n > 0 ? Math.min(Math.floor(n), MAX_LIMIT) : DEFAULT_LIMIT;
}

function encode(value: Record<string, string>): string {
  return Buffer.from(JSON.stringify(value)).toString("base64url");
}

/** The cursor's object, each of `keys` a string, or a BadRequestError. */
function decode(raw: unknown, keys: string[]): Record<string, string> {
  if (typeof raw !== "string") throw new BadRequestError("malformed cursor");
  let value: unknown;
  try {
    value = JSON.parse(Buffer.from(raw, "base64url").toString());
  } catch {
    throw new BadRequestError("cursor does not decode");
  }
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new BadRequestError("malformed cursor");
  }
  const out: Record<string, string> = {};
  for (const k of keys) {
    const v = (value as Record<string, unknown>)[k];
    if (typeof v !== "string") throw new BadRequestError("malformed cursor");
    out[k] = v;
  }
  if (!UUID.test(out.id ?? "")) throw new BadRequestError("malformed cursor id");
  return out;
}

export interface UserListItem {
  user_id: string;
  email: string;
  role: string;
  status: string;
  totp_enrolled: boolean;
  invitation: { purpose: string; expires_at: string; expired: boolean } | null;
}

/**
 * `GET /v1/users`: the org's users in `(created_at, user_id)` order, each with its open
 * invitation, if any. The cursor holds the last row's `created_at` as a count of microseconds
 * since 1970, which Postgres's time keeps exactly, and its id.
 */
export async function listUsersPage(
  pool: Pool,
  orgId: string,
  p: PageParams,
): Promise<Page<UserListItem>> {
  const limit = clampLimit(p.limit);
  const cur = p.cursor !== undefined ? decode(p.cursor, ["t", "id"]) : null;
  if (cur && !/^\d{1,17}$/.test(cur.t ?? "")) throw new BadRequestError("malformed cursor time");
  const args: unknown[] = [orgId];
  let after = "";
  if (cur) {
    args.push(cur.t, (cur.id ?? "").toLowerCase());
    after = `AND (u.created_at, u.user_id) >
                 (timestamptz 'epoch' + $2::bigint * interval '1 microsecond', $3::uuid)`;
  }
  args.push(limit + 1);
  const r = await fromStore(() =>
    pool.query<{
      user_id: string;
      email: string;
      role: string;
      status: string;
      totp_enrolled: boolean;
      micros: string;
      purpose: string | null;
      expires_at: Date | null;
      expired: boolean | null;
    }>(
      `SELECT u.user_id, u.email, u.role, u.status, u.totp_enrolled,
              (extract(epoch FROM u.created_at) * 1000000)::bigint::text AS micros,
              i.purpose, i.expires_at, i.expires_at <= now() AS expired
         FROM users u
         LEFT JOIN invitations i
           ON i.user_id = u.user_id AND i.completed_at IS NULL AND i.withdrawn_at IS NULL
        WHERE u.org_id = $1 ${after}
        ORDER BY u.created_at, u.user_id
        LIMIT $${args.length}`,
      args,
    ),
  );
  const rows = r.rows.slice(0, limit);
  const last = rows.at(-1);
  return {
    items: rows.map((row) => ({
      user_id: row.user_id,
      email: row.email,
      role: row.role,
      status: row.status,
      totp_enrolled: row.totp_enrolled,
      invitation:
        row.purpose !== null && row.expires_at !== null
          ? {
              purpose: row.purpose,
              expires_at: row.expires_at.toISOString(),
              expired: row.expired === true,
            }
          : null,
    })),
    next_cursor:
      r.rows.length > limit && last ? encode({ t: last.micros, id: last.user_id }) : null,
  };
}

/**
 * `GET /v1/users/assignable`: the org's active admins and analysts with a password and a
 * confirmed factor and no open invitation, in `(email, user_id)` order: whom an incident can be
 * assigned to (SPEC-026 §Operational §6).
 */
export async function listAssignable(
  pool: Pool,
  orgId: string,
  p: PageParams,
): Promise<Page<{ user_id: string; email: string }>> {
  const limit = clampLimit(p.limit);
  const cur = p.cursor !== undefined ? decode(p.cursor, ["e", "id"]) : null;
  if (cur && ((cur.e ?? "").length > 254 || (cur.e ?? "").includes("\u0000"))) {
    throw new BadRequestError("malformed cursor email");
  }
  const args: unknown[] = [orgId];
  let after = "";
  if (cur) {
    args.push(cur.e, (cur.id ?? "").toLowerCase());
    after = "AND (u.email, u.user_id) > ($2::text, $3::uuid)";
  }
  args.push(limit + 1);
  const r = await fromStore(() =>
    pool.query<{ user_id: string; email: string }>(
      `SELECT u.user_id, u.email
         FROM users u
        WHERE u.org_id = $1 AND u.status = 'active' AND u.role IN ('admin', 'analyst')
          AND u.password_hash IS NOT NULL AND u.totp_enrolled
          AND NOT EXISTS (SELECT 1 FROM invitations i
                           WHERE i.user_id = u.user_id
                             AND i.completed_at IS NULL AND i.withdrawn_at IS NULL)
          ${after}
        ORDER BY u.email, u.user_id
        LIMIT $${args.length}`,
      args,
    ),
  );
  const rows = r.rows.slice(0, limit);
  const last = rows.at(-1);
  return {
    items: rows.map((row) => ({ user_id: row.user_id, email: row.email })),
    next_cursor: r.rows.length > limit && last ? encode({ e: last.email, id: last.user_id }) : null,
  };
}
