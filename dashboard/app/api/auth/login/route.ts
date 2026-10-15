import { apiBaseUrl } from "@/lib/api/client";
import { httpAuthApi } from "@/lib/auth/api";
import { handleLogin } from "@/lib/auth/handlers";

/** `POST /api/auth/login` (SPEC-009's second Amendment 2026-10-15, item 3). */
export async function POST(req: Request): Promise<Response> {
  return handleLogin(req, httpAuthApi(apiBaseUrl()));
}
