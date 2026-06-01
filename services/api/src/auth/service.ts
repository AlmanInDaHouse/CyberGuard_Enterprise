import { NotImplementedError } from "../errors.js";

/**
 * SPEC-008 auth-core service contract. Every method is the realisation surface
 * the GREEN gate fills; during the harness-first RED gate the stub below throws
 * `NotImplementedError` for each, so every auth AC fails RED because the CONTROL
 * is absent (not because setup is broken). The contracts (login flow, session,
 * RBAC, rate-limit, CSRF, audit, user management) are SPEC-008 §Operational.
 */
export interface LoginInput {
  email: string;
  password: string;
  totp_code: string;
}

export interface SessionInfo {
  user_id: string;
  org_id: string;
  role: string;
  csrf_token: string;
}

export interface LoginResult {
  token: string;
  csrfToken: string;
  session: SessionInfo;
}

export interface CreateUserInput {
  email: string;
  password: string;
  role: string;
  org_id?: string;
}

export interface AuthService {
  /** password (Argon2) → TOTP (RFC 6238, replay-rejected) → opaque Redis session + cookie + CSRF. */
  login(input: LoginInput): Promise<LoginResult>;
  /** Revoke the session (Redis DEL) — immediate, the product invariant. */
  logout(token: string): Promise<void>;
  /** Resolve a session token to its server-side authority (role/org); null if absent/revoked. */
  resolveSession(token: string): Promise<SessionInfo | null>;
  /** Admin-only: create a user (Argon2 hash + pgcrypto-encrypted TOTP secret). */
  createUser(input: CreateUserInput): Promise<{ userId: string; totpProvisioningUri: string }>;
  /** Admin-only: list users. */
  listUsers(): Promise<Array<{ userId: string; email: string; role: string }>>;
  /** Admin-only: change a user's role (rotates that user's sessions). */
  assignRole(userId: string, role: string): Promise<void>;
  /** Authenticated self-service: change own password (rotates the session). */
  changePassword(input: {
    userId: string;
    currentPassword: string;
    newPassword: string;
    totp_code: string;
  }): Promise<void>;
  /** Authenticated self-service: confirm first-enrollment TOTP, flipping totp_enrolled. */
  confirmTotpEnrollment(input: { userId: string; totp_code: string }): Promise<void>;
}

/** The RED-gate stub: every method throws NotImplementedError. */
export function notImplementedAuthService(): AuthService {
  return {
    login: () => {
      throw new NotImplementedError("auth.login");
    },
    logout: () => {
      throw new NotImplementedError("auth.logout");
    },
    resolveSession: () => {
      throw new NotImplementedError("auth.resolveSession");
    },
    createUser: () => {
      throw new NotImplementedError("auth.createUser");
    },
    listUsers: () => {
      throw new NotImplementedError("auth.listUsers");
    },
    assignRole: () => {
      throw new NotImplementedError("auth.assignRole");
    },
    changePassword: () => {
      throw new NotImplementedError("auth.changePassword");
    },
    confirmTotpEnrollment: () => {
      throw new NotImplementedError("auth.confirmTotpEnrollment");
    },
  };
}
