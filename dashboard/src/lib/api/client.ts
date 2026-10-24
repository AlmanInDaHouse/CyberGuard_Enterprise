/** A minimal seam over the read-API HTTP surface. Production uses `httpApiClient`
 *  (real `fetch`); the dash_ac_001 integration test passes an adapter that routes
 *  to services/api's `app.inject` in-process (no socket, real handlers). The
 *  data-access layer depends only on this interface, never on `fetch` directly. */
export interface ApiResponse {
  status: number;
  body: unknown;
}

export interface ApiClient {
  get(path: string, cookieHeader: string | undefined): Promise<ApiResponse>;
}

/** Why `API_BASE_URL` is refused (SPEC-023 §Data contracts §3): set, it is an `http:` or
 *  `https:` URL, and not the empty string; one that carries credentials is refused too,
 *  since `fetch` refuses every call to it. Null when it is unset or such a URL. The reason
 *  does not quote the value. */
export function apiBaseUrlProblem(env: Record<string, string | undefined>): string | null {
  const value = env.API_BASE_URL;
  if (value === undefined) return null;
  if (value === "") return "must not be empty";
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    return "must be an http: or https: URL";
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    return "must be an http: or https: URL";
  }
  return url.username === "" && url.password === "" ? null : "must not carry credentials";
}

/** The read-API's base URL: `API_BASE_URL` when set, else the api's own default
 *  listener (`API_BIND_HOST` 127.0.0.1 and `API_PORT` 8081, services/api/src/config.ts).
 *  The server checks it at its start (`instrumentation.ts`). */
export function apiBaseUrl(env: Record<string, string | undefined> = process.env): string {
  return env.API_BASE_URL ?? "http://127.0.0.1:8081";
}

/** The production client: sends the `Cookie` header it is given, the session alone
 *  (`sessionCookieHeader`, SPEC-009's second Amendment 2026-10-15, item 6), to the
 *  read-API over HTTP, never caching (every read reflects current auth + data). */
export function httpApiClient(baseUrl: string): ApiClient {
  return {
    async get(path, cookieHeader) {
      const res = await fetch(new URL(path, baseUrl), {
        headers: cookieHeader ? { cookie: cookieHeader } : {},
        cache: "no-store",
      });
      const body = res.status === 200 ? await res.json() : undefined;
      return { status: res.status, body };
    },
  };
}
