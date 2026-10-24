import { type NextRequest, NextResponse } from "next/server";
import {
  STATIC_SECURITY_HEADERS,
  contentSecurityPolicy,
  newNonce,
} from "./src/lib/security-headers";

/**
 * SPEC-023 §Operational §7 (debt #104): every response carries the security headers, and,
 * but under `next dev`, the CSP with a nonce new per response. The CSP goes on the request
 * too: Next reads the nonce from it and puts it on the scripts it renders.
 */
export function middleware(request: NextRequest): NextResponse {
  const csp =
    process.env.NODE_ENV === "development" ? undefined : contentSecurityPolicy(newNonce());
  const requestHeaders = new Headers(request.headers);
  if (csp) requestHeaders.set("Content-Security-Policy", csp);
  const response = NextResponse.next({ request: { headers: requestHeaders } });
  if (csp) response.headers.set("Content-Security-Policy", csp);
  for (const [name, value] of Object.entries(STATIC_SECURITY_HEADERS)) {
    response.headers.set(name, value);
  }
  return response;
}
