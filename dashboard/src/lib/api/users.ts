import type { UserItem } from "@/components/users-admin";
import type { ApiClient } from "./client";
import type { ReadResult } from "./result";
import type { Page } from "./types";

/** `GET /v1/users` (SPEC-026 §Data contracts §3), an admin's: a page of the org's users after
 *  `cursor`, the api's opaque text, which the page passes on as it came. */
export async function getUsers(
  client: ApiClient,
  cookieHeader: string | undefined,
  cursor: string | undefined,
): Promise<ReadResult<Page<UserItem>>> {
  const path =
    cursor === undefined ? "/v1/users" : `/v1/users?cursor=${encodeURIComponent(cursor)}`;
  const res = await client.get(path, cookieHeader);
  if (res.status === 401) return { ok: false, reason: "unauthenticated" };
  if (res.status === 403) return { ok: false, reason: "forbidden" };
  if (res.status === 400) return { ok: false, reason: "bad_request" };
  if (res.status !== 200) throw new Error(`read-API GET /v1/users: ${res.status}`);
  return { ok: true, data: res.body as Page<UserItem> };
}
