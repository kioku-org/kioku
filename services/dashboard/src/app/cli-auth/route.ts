import { NextRequest, NextResponse } from "next/server";
import { getServerSession } from "next-auth";
import { authOptions } from "@/app/api/auth/[...nextauth]/route";
import { CALENDAR_SCOPE, signCalendarState } from "@/lib/google-calendar-cli-state";

export async function GET(request: NextRequest) {
  const { searchParams } = new URL(request.url);
  const port = searchParams.get("port");
  const state = searchParams.get("state");
  const provider = searchParams.get("provider") ?? "google";

  const portNum = parseInt(port ?? "", 10);
  if (!port || isNaN(portNum) || portNum < 1024 || portNum > 65535) {
    return new NextResponse("Invalid or missing port parameter.", { status: 400 });
  }
  if (!state || state.length < 8) {
    return new NextResponse("Invalid or missing state parameter.", { status: 400 });
  }

  const session = await getServerSession(authOptions);

  // For the CLI, picking Google always means "and Calendar access too" —
  // there's no separate `kioku cal` connect step later. GitHub can't grant
  // Calendar, so it's untouched.
  const wantsCalendar = provider !== "github";

  const email = session?.user?.email;
  const hivemindToken: string | undefined = (session as any)?.hivemindToken;

  let identityOk = false;
  let name = email?.split("@")[0] ?? "";
  let workspaceId: string | undefined;
  let role: string | undefined;
  let userId: string | undefined;
  if (hivemindToken) {
    try {
      const payload = JSON.parse(Buffer.from(hivemindToken.split(".")[1], "base64url").toString());
      if (payload.user_id && payload.workspace_id) {
        identityOk = true;
        userId = payload.user_id;
        workspaceId = payload.workspace_id;
        role = payload.role ?? "member";
        name = payload.name ?? name;
      }
    } catch {
      // Malformed token — treat as unusable, fall through.
    }
  }

  // Deliberately anchored to NEXTAUTH_URL rather than `request.url`: behind
  // this deployment's reverse proxy, `request.url`'s origin resolves to the
  // container's internal bind address (0.0.0.0:3001) instead of the public
  // hostname, which produces an unreachable redirect for the browser.
  const base = process.env.NEXTAUTH_URL || request.url;

  if (!email || !identityOk) {
    // No usable identity at all — go through normal NextAuth sign-in.
    // Comes back here once done, at which point identity is established
    // and (if Google) the branch below picks up the Calendar leg.
    const self = `/cli-auth?port=${port}&state=${encodeURIComponent(state)}&provider=${provider}`;
    const nextAuthProviderId = provider === "github" ? "github" : "google";
    return NextResponse.redirect(
      new URL(`/api/auth/signin/${nextAuthProviderId}?callbackUrl=${encodeURIComponent(self)}`, base)
    );
  }

  if (wantsCalendar) {
    // Identity is established, but we still need a Calendar grant on this
    // browser's Google session. NextAuth's /api/auth/signin/[provider] is
    // a dead end here: it short-circuits to "already signed in" and skips
    // re-authentication entirely when a session exists, regardless of
    // which provider was requested — it can't be used to step up scope on
    // an existing session. So this talks to Google directly instead,
    // server-side, using the dashboard's own confidential Google client;
    // the CLI never sees or holds that client's credentials.
    const clientId = process.env.GOOGLE_CLIENT_ID;
    if (!clientId) {
      return new NextResponse("Google OAuth is not configured.", { status: 500 });
    }

    const cliState = signCalendarState({
      port: portNum,
      cliState: state,
      exp: Math.floor(Date.now() / 1000) + 600,
    });

    const authUrl = new URL("https://accounts.google.com/o/oauth2/v2/auth");
    authUrl.searchParams.set("client_id", clientId);
    authUrl.searchParams.set("redirect_uri", `${base}/cli-auth/calendar-callback`);
    authUrl.searchParams.set("response_type", "code");
    authUrl.searchParams.set("scope", CALENDAR_SCOPE);
    authUrl.searchParams.set("access_type", "offline");
    authUrl.searchParams.set("prompt", "consent");
    authUrl.searchParams.set("login_hint", email);
    authUrl.searchParams.set("state", cliState);
    return NextResponse.redirect(authUrl.toString());
  }

  // GitHub, or Calendar not requested — hand off to the CLI loopback now.
  const params = new URLSearchParams({
    token: hivemindToken!,
    state,
    user_id: userId!,
    email,
    name,
    workspace_id: workspaceId!,
    role: role!,
  });
  return NextResponse.redirect(`http://localhost:${portNum}/callback?${params}`);
}
