import { apiBaseUrl } from "@/lib/api/client";
import { httpAuthApi } from "@/lib/auth/api";
import { MUTATIONS, handleMutation } from "@/lib/auth/mutations";

/** `POST /api/users`, the api's `POST /v1/users` (SPEC-026 §Operational §9). */
export async function POST(req: Request): Promise<Response> {
  return handleMutation(req, httpAuthApi(apiBaseUrl()), MUTATIONS.createUser);
}
