import crypto from "crypto";

export const ADMIN_COOKIE_NAME = "kioku-admin-session";
export const ADMIN_COOKIE_MAX_AGE = 60 * 60 * 24; // 24 hours

export function getSigningSecret(): string {
  return process.env.JWT_SECRET || process.env.VEXA_ADMIN_API_KEY || "default-secret-change-me";
}

export function signCookieValue(payload: string): string {
  const hmac = crypto.createHmac("sha256", getSigningSecret()).update(payload).digest("hex");
  return `${payload}.${hmac}`;
}

export function verifyCookieValue(signed: string): string | null {
  const dotIndex = signed.lastIndexOf(".");
  if (dotIndex === -1) return null;
  const payload = signed.substring(0, dotIndex);
  const signature = signed.substring(dotIndex + 1);
  const expected = crypto.createHmac("sha256", getSigningSecret()).update(payload).digest("hex");
  if (signature.length !== expected.length ||
      !crypto.timingSafeEqual(Buffer.from(signature), Buffer.from(expected))) {
    return null;
  }
  return payload;
}

export function isAdminSessionPayloadValid(payload: string): boolean {
  const sessionData = JSON.parse(Buffer.from(payload, "base64").toString());
  const sessionAge = Date.now() - sessionData.timestamp;
  if (sessionAge > ADMIN_COOKIE_MAX_AGE * 1000) {
    return false;
  }
  return sessionData.authenticated === true;
}
