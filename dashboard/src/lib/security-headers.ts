/**
 * SPEC-023 §Operational §7 (debt #104): the headers on every response of the dashboard's
 * server. The CSP's script-src admits the response's nonce, which Next puts on the scripts
 * it renders, and what those scripts load (`'strict-dynamic'`), never `'unsafe-inline'`.
 * HSTS is the proxy's (§Operational §5). `next dev` gets no CSP: React's development build
 * evaluates code.
 */
export function contentSecurityPolicy(nonce: string): string {
  return [
    "default-src 'self'",
    `script-src 'self' 'nonce-${nonce}' 'strict-dynamic'`,
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data:",
    "connect-src 'self'",
    "frame-ancestors 'none'",
    "base-uri 'none'",
    "form-action 'self'",
    "object-src 'none'",
  ].join("; ");
}

/** The headers besides the CSP. `same-origin`, not `no-referrer`, under which a browser
 *  sends `Origin: null` and the sign-in's same-origin check would refuse it. */
export const STATIC_SECURITY_HEADERS: Readonly<Record<string, string>> = Object.freeze({
  "X-Content-Type-Options": "nosniff",
  "Referrer-Policy": "same-origin",
  "Permissions-Policy": "camera=(), microphone=(), geolocation=()",
});

/** A nonce: 16 random bytes in base64. */
export function newNonce(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return btoa(String.fromCharCode(...bytes));
}
