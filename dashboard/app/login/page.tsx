import { LoginForm } from "@/components/login-form";

/** Login view (SPEC-009 §Operational §3; its second Amendment 2026-10-15, item 7). The
 *  sign-out that could not reach the api arrives with `?notice=signed-out`. */
export default async function LoginPage({
  searchParams,
}: {
  searchParams: Promise<{ notice?: string | string[] }>;
}) {
  const { notice } = await searchParams;
  return <LoginForm signedOutNotice={notice === "signed-out"} />;
}
