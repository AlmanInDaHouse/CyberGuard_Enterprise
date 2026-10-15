/** The cookies the dashboard's origin holds (SPEC-009's second Amendment 2026-10-15,
 *  the dashboard's login and logout): the api's session token and its CSRF token, both
 *  `HttpOnly`, so no script of the page reads either. */
export const SESSION_COOKIE = "cgsess";
export const CSRF_COOKIE = "cgcsrf";

const ATTRIBUTES = "HttpOnly; Secure; SameSite=Strict; Path=/";

/** A token as the api issues them, base64url without padding
 *  (`services/api/src/auth/session.ts`). The dashboard sets no cookie value of another
 *  shape, so such a value is no session; read as one, some values, once decoded, could
 *  not go into the header that carries them. */
export const TOKEN = /^[A-Za-z0-9_-]+$/;

/** A cookie value when it has a token's shape. */
export function tokenShaped(value: string | undefined): string | undefined {
  return value !== undefined && TOKEN.test(value) ? value : undefined;
}

/** A `Set-Cookie` value that holds a cookie until the browser's session ends (item 3). */
export function setCookie(name: string, value: string): string {
  return `${name}=${value}; ${ATTRIBUTES}`;
}

/** A `Set-Cookie` value that clears a cookie (item 4). */
export function clearCookie(name: string): string {
  return `${name}=; ${ATTRIBUTES}; Max-Age=0`;
}

/** What a page reads from the browser's cookies: Next's `cookies()` store has `get`. */
export interface CookieReader {
  get(name: string): { value: string } | undefined;
}

/**
 * The `Cookie` header a page sends to the read API: the session alone, and none when
 * the browser holds no session (item 6, debt #97).
 */
export function sessionCookieHeader(store: CookieReader): string | undefined {
  const token = tokenShaped(store.get(SESSION_COOKIE)?.value);
  return token ? `${SESSION_COOKIE}=${token}` : undefined;
}
