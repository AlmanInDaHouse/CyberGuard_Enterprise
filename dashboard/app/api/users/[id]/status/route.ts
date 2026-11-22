import { apiBaseUrl } from "@/lib/api/client";
import { httpAuthApi } from "@/lib/auth/api";
import { MUTATIONS, handleMutation } from "@/lib/auth/mutations";

/** `POST /api/users/[id]/status`, the api's `POST /v1/users/:id/status` (SPEC-026 §Operational §9). */
export async function POST(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const { id } = await ctx.params;
  return handleMutation(req, httpAuthApi(apiBaseUrl()), MUTATIONS.userStatus, id);
}
