import { createHmac } from "crypto";

export const CALENDAR_SCOPE =
  "openid email profile https://www.googleapis.com/auth/calendar.readonly";

export type CalendarCliState = {
  port: number;
  cliState: string;
  exp: number;
};

function stateSecret(): string {
  return process.env.NEXTAUTH_SECRET || process.env.VEXA_ADMIN_API_KEY || "";
}

/// Signs the CLI's loopback port + its own CSRF state so they survive the
/// round trip through Google's `state` param, tamper-evident (HMAC) and
/// time-limited. Unlike the dashboard's separate, unrelated
/// `api/calendar/oauth/*` scaffold, this is only ever minted from a request
/// that already has a verified NextAuth session (see cli-auth/route.ts) —
/// there's no user-supplied identity to spoof.
export function signCalendarState(payload: CalendarCliState): string {
  const data = Buffer.from(JSON.stringify(payload)).toString("base64url");
  const signature = createHmac("sha256", stateSecret()).update(data).digest("base64url");
  return `${data}.${signature}`;
}

export function verifyCalendarState(state: string): CalendarCliState {
  const [data, signature] = state.split(".");
  if (!data || !signature) {
    throw new Error("invalid state format");
  }
  const expected = createHmac("sha256", stateSecret()).update(data).digest("base64url");
  if (signature !== expected) {
    throw new Error("state signature mismatch");
  }
  const payload = JSON.parse(Buffer.from(data, "base64url").toString()) as CalendarCliState;
  if (!payload.exp || payload.exp < Math.floor(Date.now() / 1000)) {
    throw new Error("state expired");
  }
  if (!payload.port || !payload.cliState) {
    throw new Error("state missing required fields");
  }
  return payload;
}
