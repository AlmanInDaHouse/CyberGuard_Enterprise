import { AppShell } from "@/components/app-nav";
import { PasswordChangeForm } from "@/components/password-change-form";
import { signedIn } from "@/lib/signed-in";

export const dynamic = "force-dynamic";

/** `/account` (SPEC-026 §Operational §9, phase 1): the password change, for every role. */
export default async function AccountPage() {
  const { user } = await signedIn();
  return (
    <AppShell user={user}>
      <h1 className="mb-4 text-xl font-semibold">Account</h1>
      <p className="mb-4 text-sm text-slate-700">Role: {user.role}</p>
      <PasswordChangeForm />
    </AppShell>
  );
}
