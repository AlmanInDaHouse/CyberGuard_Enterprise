import { apiBaseUrl } from "@/lib/api/client";
import { httpAuthApi } from "@/lib/auth/api";
import { handleLogout } from "@/lib/auth/handlers";

/** `POST /api/auth/logout` (SPEC-009's second Amendment 2026-10-15, item 4). */
export async function POST(req: Request): Promise<Response> {
  return handleLogout(req, httpAuthApi(apiBaseUrl()));
}
