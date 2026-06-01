// SPEC-008 §Operational §6 — first-admin bootstrap CLI (`task api:create-user`).
// Seeds the first `admin` directly in Postgres (Argon2 hash + pgcrypto-encrypted
// TOTP secret + a printed provisioning URI) so user creation, which is otherwise
// admin-only over the authed API, has no unauthenticated HTTP bootstrap path.
// The logic lands at the GREEN gate; the RED stub throws.
import { NotImplementedError } from "../errors.js";

async function main(): Promise<void> {
  throw new NotImplementedError("cli.create-user (first-admin bootstrap)");
}

main().catch((e: unknown) => {
  process.stderr.write(`cg-api create-user: ${String(e)}\n`);
  process.exit(1);
});
