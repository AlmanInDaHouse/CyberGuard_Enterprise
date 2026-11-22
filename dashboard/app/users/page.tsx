import { AppShell } from "@/components/app-nav";
import { UsersAdmin } from "@/components/users-admin";
import { getUsers } from "@/lib/api/users";
import { signedIn } from "@/lib/signed-in";
import { notFound, redirect } from "next/navigation";

export const dynamic = "force-dynamic";

/** `/users` (SPEC-026 §Operational §9, phase 1), for admins: the org's users, a page at a time
 *  by the api's cursor. Another role's request is not found and reads no user; the api's checks
 *  decide (its 403 is not found too). */
export default async function UsersPage({
  searchParams,
}: {
  searchParams: Promise<{ cursor?: string | string[] }>;
}) {
  const { cursor: raw } = await searchParams;
  const cursor = typeof raw === "string" && raw !== "" ? raw : undefined;
  const { user, client, cookieHeader } = await signedIn();
  if (user.role !== "admin") notFound();
  const result = await getUsers(client, cookieHeader, cursor);
  if (!result.ok) {
    if (result.reason === "unauthenticated") redirect("/login");
    if (result.reason === "bad_request") redirect("/users");
    notFound();
  }

  return (
    <AppShell user={user}>
      <h1 className="mb-4 text-xl font-semibold">Users</h1>
      <UsersAdmin page={result.data} selfId={user.user_id} cursor={cursor} />
    </AppShell>
  );
}
