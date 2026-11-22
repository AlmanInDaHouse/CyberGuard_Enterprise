import { apiBaseUrl, httpApiClient } from "@/lib/api/client";
import type { ApiClient } from "@/lib/api/client";
import { type SessionUser, getSessionUser } from "@/lib/api/session";
import { sessionCookieHeader } from "@/lib/auth/cookies";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";

/**
 * A signed-in page's first read (SPEC-026 §Operational §9): the session's user, from
 * `GET /v1/auth/session` with the session cookie alone (SPEC-009's second Amendment
 * 2026-10-15, item 6); without a session the api holds, the login. Returns the user, the
 * client and the `Cookie` header the page's other reads send.
 */
export async function signedIn(): Promise<{
  user: SessionUser;
  client: ApiClient;
  cookieHeader: string | undefined;
}> {
  const cookieHeader = sessionCookieHeader(await cookies());
  const client = httpApiClient(apiBaseUrl());
  const result = await getSessionUser(client, cookieHeader);
  if (!result.ok) redirect("/login");
  return { user: result.data, client, cookieHeader };
}
