import type { ApiClient } from "./client";
import type { ReadResult } from "./result";

/** `GET /v1/auth/session` (SPEC-026 §Data contracts §3): the session's user. Its role shows or
 *  hides a page's controls; the api's checks decide (§Operational §9). */
export interface SessionUser {
  user_id: string;
  email: string;
  role: string;
}

export async function getSessionUser(
  client: ApiClient,
  cookieHeader: string | undefined,
): Promise<ReadResult<SessionUser>> {
  const res = await client.get("/v1/auth/session", cookieHeader);
  if (res.status === 401) return { ok: false, reason: "unauthenticated" };
  if (res.status !== 200) throw new Error(`read-API GET /v1/auth/session: ${res.status}`);
  return { ok: true, data: res.body as SessionUser };
}
