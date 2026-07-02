import { NextRequest, NextResponse } from "next/server";
import { getServerSession } from "next-auth";
import { authOptions } from "@/app/api/auth/[...nextauth]/route";
import { verifyCalendarState } from "@/lib/google-calendar-cli-state";

export async function GET(request: NextRequest) {
  const { searchParams } = new URL(request.url);
  const code = searchParams.get("code");
  const state = searchParams.get("state");
  const error = searchParams.get("error");

  if (error) {
    return new NextResponse(`Google reported an error: ${error}`, { status: 400 });
  }
  if (!code || !state) {
    return new NextResponse("Missing code or state.", { status: 400 });
  }

  let payload: { port: number; cliState: string };
  try {
    payload = verifyCalendarState(state);
  } catch (e) {
    return new NextResponse(`Invalid state: ${(e as Error).message}`, { status: 400 });
  }

  const session = await getServerSession(authOptions);
  const hivemindToken: string | undefined = (session as any)?.hivemindToken;
  const email = session?.user?.email;
  if (!email || !hivemindToken) {
    return new NextResponse("Session expired — please run `kioku signin` again.", { status: 401 });
  }

  let userId: string | undefined;
  let workspaceId: string | undefined;
  let role: string | undefined;
  let name: string | undefined;
  try {
    const jwtPayload = JSON.parse(Buffer.from(hivemindToken.split(".")[1], "base64url").toString());
    userId = jwtPayload.user_id;
    workspaceId = jwtPayload.workspace_id;
    role = jwtPayload.role ?? "member";
    name = jwtPayload.name ?? email.split("@")[0];
  } catch {
    return new NextResponse("Corrupt session token — please run `kioku signin` again.", { status: 401 });
  }
  if (!userId || !workspaceId) {
    return new NextResponse(
      "Session missing required fields — please run `kioku signin` again.",
      { status: 401 }
    );
  }

  const clientId = process.env.GOOGLE_CLIENT_ID;
  const clientSecret = process.env.GOOGLE_CLIENT_SECRET;
  if (!clientId || !clientSecret) {
    return new NextResponse("Google OAuth is not configured.", { status: 500 });
  }

  const base = process.env.NEXTAUTH_URL || request.url;
  const redirectUri = `${base}/cli-auth/calendar-callback`;

  const tokenRes = await fetch("https://oauth2.googleapis.com/token", {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({
      code,
      client_id: clientId,
      client_secret: clientSecret,
      redirect_uri: redirectUri,
      grant_type: "authorization_code",
    }),
    cache: "no-store",
  });

  if (!tokenRes.ok) {
    const text = await tokenRes.text();
    return new NextResponse(`Google token exchange failed: ${text}`, { status: 502 });
  }

  const tokens = (await tokenRes.json()) as {
    access_token: string;
    refresh_token?: string;
    expires_in: number;
  };
  if (!tokens.refresh_token) {
    return new NextResponse(
      "Google didn't return a refresh token. If you've granted Kioku access before, " +
        "revoke it at https://myaccount.google.com/permissions and try again.",
      { status: 400 }
    );
  }

  const params = new URLSearchParams({
    token: hivemindToken,
    state: payload.cliState,
    user_id: userId,
    email,
    name: name!,
    workspace_id: workspaceId,
    role: role!,
    google_access_token: tokens.access_token,
    google_refresh_token: tokens.refresh_token,
    google_token_expires_at: String(Math.floor(Date.now() / 1000) + tokens.expires_in),
  });
  return NextResponse.redirect(`http://localhost:${payload.port}/callback?${params}`);
}
