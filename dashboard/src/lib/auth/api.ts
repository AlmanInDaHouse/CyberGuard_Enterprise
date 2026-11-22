/** The api's routes the dashboard's handlers call: the login and the logout (SPEC-009's
 *  second Amendment 2026-10-15, item 1) and the mutations (SPEC-026 §Operational §9,
 *  `src/lib/auth/mutations.ts`). Production uses `httpAuthApi`; the tests pass an adapter
 *  over services/api's `app.inject`, or a recording client. */
export type AuthPath = `/v1/${string}`;

export interface AuthApiResponse {
  status: number;
  /** The response's `Set-Cookie` values. */
  setCookie: string[];
  /** The JSON body, or `undefined` when it is not JSON. */
  body: unknown;
}

export interface AuthApi {
  /** Rejects when the api does not answer: a connection refused or reset, or no
   *  answer within the bound. */
  post(path: AuthPath, headers: Record<string, string>, body?: string): Promise<AuthApiResponse>;
}

/** Each call to the api is bounded to 30 seconds (item 1). */
export const AUTH_CALL_BOUND_MS = 30_000;

export function httpAuthApi(baseUrl: string, boundMs: number = AUTH_CALL_BOUND_MS): AuthApi {
  return {
    async post(path, headers, body) {
      const res = await fetch(new URL(path, baseUrl), {
        method: "POST",
        headers,
        body,
        cache: "no-store",
        redirect: "manual",
        signal: AbortSignal.timeout(boundMs),
      });
      const text = await res.text();
      let parsed: unknown;
      try {
        parsed = JSON.parse(text);
      } catch {
        parsed = undefined;
      }
      return { status: res.status, setCookie: res.headers.getSetCookie(), body: parsed };
    },
  };
}
