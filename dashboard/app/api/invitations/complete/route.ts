import { apiBaseUrl } from "@/lib/api/client";
import { httpAuthApi } from "@/lib/auth/api";
import { MUTATIONS, handleMutation } from "@/lib/auth/mutations";

/** `POST /api/invitations/complete`, the api's `POST /v1/invitations/complete` (SPEC-026 §Operational §9). */
export async function POST(req: Request): Promise<Response> {
  return handleMutation(req, httpAuthApi(apiBaseUrl()), MUTATIONS.invitationComplete);
}
