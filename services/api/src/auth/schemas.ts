import { z } from "zod";
import { BadRequestError } from "../errors.js";

// SPEC-008 Amendment 2026-10-15 item 2 — the strict schemas of SPEC-008's routes. An
// extra field, a missing one or one of the wrong type is refused; lengths are
// JavaScript string lengths (UTF-16 code units) except a new password's (item 4).

const ROLES = ["admin", "analyst", "viewer"] as const;
const STATUSES = ["active", "disabled"] as const;

/** A UUID in canonical text form, either case (SPEC-009 Amendment 2026-10-14 item 1). */
const CANONICAL_UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** A canonical UUID, read in lower case. */
const userId = z
  .string()
  .regex(CANONICAL_UUID)
  .transform((s) => s.toLowerCase());

const totpCode = z.string().regex(/^[0-9]{6}$/);

/** A password at the login or a current password: whatever was set, bounded. */
const anyPassword = z.string().min(1).max(1024);

/**
 * A new password (item 4): 12 to 128 Unicode code points, with at least one letter and
 * one decimal digit (PCI DSS v4.0 8.3.6; OWASP ASVS 4.0 V2.1.1, V2.1.2).
 */
export const newPassword = z.string().refine((s) => {
  const points = Array.from(s).length;
  return points >= 12 && points <= 128 && /\p{L}/u.test(s) && /\p{Nd}/u.test(s);
});

/**
 * The login's address: 1 to 254 once trimmed, with no NUL, which Postgres cannot take
 * as text (Amendment item 2). The service trims and lower-cases it (item 3).
 */
const loginEmail = z.string().refine((s) => {
  const t = s.trim().toLowerCase();
  return t.length >= 1 && t.length <= 254 && !t.includes("\u0000");
});

export const LoginBody = z
  .object({ email: loginEmail, password: anyPassword, totp_code: totpCode })
  .strict();

export const TotpConfirmBody = z.object({ userId, totp_code: totpCode }).strict();

export const PasswordBody = z
  .object({ currentPassword: anyPassword, newPassword, totp_code: totpCode })
  .strict();

export const CreateUserBody = z
  .object({
    email: z.string().max(254).email(),
    password: newPassword,
    role: z.enum(ROLES),
  })
  .strict();

export const RoleBody = z.object({ role: z.enum(ROLES) }).strict();

export const StatusBody = z.object({ status: z.enum(STATUSES) }).strict();

/** Parse a body with its schema; a body that fails is a BadRequestError (400). */
export function parseBody<S extends z.ZodTypeAny>(schema: S, body: unknown): z.infer<S> {
  const parsed = schema.safeParse(body);
  if (!parsed.success) throw new BadRequestError();
  return parsed.data;
}

/** A canonical UUID read in lower case, or null for any other `:id`. */
export function canonicalId(raw: string): string | null {
  return CANONICAL_UUID.test(raw) ? raw.toLowerCase() : null;
}
