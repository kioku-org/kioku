import { NextRequest, NextResponse } from "next/server";

/// Refreshes a Google Calendar access token on behalf of the `kioku` CLI.
///
/// The CLI holds a Calendar refresh_token issued to this dashboard's
/// confidential Google OAuth client (see the `google-calendar` NextAuth
/// provider and /cli-auth) — redeeming it requires that client's secret,
/// which must stay server-side. The CLI authenticates with its normal
/// Kioku bearer token (verified against Hivemind) rather than any Google
/// credential of its own.
export async function POST(request: NextRequest) {
  const auth = request.headers.get("authorization");
  if (!auth?.startsWith("Bearer ")) {
    return NextResponse.json({ error: "Missing Authorization header" }, { status: 401 });
  }

  const hivemindUrl = process.env.HIVEMIND_INTERNAL_URL ?? "http://localhost:9100";
  const meRes = await fetch(`${hivemindUrl}/auth/me`, {
    headers: { Authorization: auth },
    cache: "no-store",
  });
  if (!meRes.ok) {
    return NextResponse.json({ error: "Invalid or expired token" }, { status: 401 });
  }

  const { refresh_token } = (await request.json().catch(() => ({}))) as {
    refresh_token?: string;
  };
  if (!refresh_token) {
    return NextResponse.json({ error: "refresh_token is required" }, { status: 400 });
  }

  const clientId = process.env.GOOGLE_CLIENT_ID;
  const clientSecret = process.env.GOOGLE_CLIENT_SECRET;
  if (!clientId || !clientSecret) {
    return NextResponse.json({ error: "Google OAuth is not configured" }, { status: 500 });
  }

  const tokenRes = await fetch("https://oauth2.googleapis.com/token", {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({
      client_id: clientId,
      client_secret: clientSecret,
      refresh_token,
      grant_type: "refresh_token",
    }),
    cache: "no-store",
  });

  if (!tokenRes.ok) {
    const text = await tokenRes.text();
    return NextResponse.json(
      { error: `Google refresh failed: ${text}` },
      { status: tokenRes.status }
    );
  }

  const data = (await tokenRes.json()) as { access_token: string; expires_in: number };
  return NextResponse.json({ access_token: data.access_token, expires_in: data.expires_in });
}
