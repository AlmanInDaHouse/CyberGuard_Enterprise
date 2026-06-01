import otplib from "otplib";

const { authenticator } = otplib;

// RFC 6238 TOTP (otplib): 30 s step, 6 digits. `window: 1` tolerates a ±1-step
// clock skew at verification so a code generated at a step boundary still
// verifies. Replay rejection is enforced separately (the service tracks the
// accepted code per user in Redis), so a wider verify window does not weaken it.
authenticator.options = { window: 1 };

export function generateSecret(): string {
  return authenticator.generateSecret();
}

export function verifyTotp(secret: string, token: string): boolean {
  try {
    return authenticator.verify({ token, secret });
  } catch {
    return false;
  }
}

export function provisioningUri(account: string, secret: string): string {
  return authenticator.keyuri(account, "CyberGuard", secret);
}
