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

/** The production client: forwards the server-side `cgsess` cookie to the read-API
 *  over HTTP, never caching (every read reflects current auth + data). */
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
