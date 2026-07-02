import { NextRequest, NextResponse } from "next/server";
import { getServerSession } from "next-auth";
import { authOptions } from "@/app/api/auth/[...nextauth]/route";

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
  // Calendar, so it's untouched. This only affects the CLI's /cli-auth
  // flow — the web /login page still uses the plain "google" provider
  // (default scope), so ordinary dashboard users are never prompted for
  // Calendar access they didn't ask for.
  const wantsCalendar = provider !== "github";
  const nextAuthProviderId = wantsCalendar ? "google-calendar" : "github";

  const email = session?.user?.email;
  const hivemindToken: string | undefined = (session as any)?.hivemindToken;
  const googleAccessToken: string | undefined = (session as any)?.googleAccessToken;
  const googleRefreshToken: string | undefined = (session as any)?.googleRefreshToken;
  const googleTokenExpiresAt: number | undefined = (session as any)?.googleTokenExpiresAt;

  // Decide whether the current session (if any) is actually usable for
  // this request: valid post-rename identity token, and — when the CLI
  // wants Calendar — a Calendar-scoped Google grant too. A session missing
  // either falls through to a fresh OAuth round trip rather than being
  // used half-broken.
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

  const calendarOk = !wantsCalendar || !!(googleAccessToken && googleRefreshToken);

  if (!email || !identityOk || !calendarOk) {
    // Not signed in, or signed in without what this request needs — send
    // to the OAuth provider, come back here after. Deliberately anchored
    // to NEXTAUTH_URL rather than `request.url`: behind this deployment's
    // reverse proxy, `request.url`'s origin resolves to the container's
    // internal bind address (0.0.0.0:3001) instead of the public hostname,
    // which produces an unreachable redirect for the browser.
    const self = `/cli-auth?port=${port}&state=${encodeURIComponent(state)}&provider=${provider}`;
    const base = process.env.NEXTAUTH_URL || request.url;
    return NextResponse.redirect(
      new URL(`/api/auth/signin/${nextAuthProviderId}?callbackUrl=${encodeURIComponent(self)}`, base)
    );
  }

  const params = new URLSearchParams({
    token: hivemindToken!,
    state,
    user_id: userId!,
    email,
    name,
    workspace_id: workspaceId!,
    role: role!,
  });
  if (wantsCalendar && googleAccessToken && googleRefreshToken) {
    params.set("google_access_token", googleAccessToken);
    params.set("google_refresh_token", googleRefreshToken);
    if (googleTokenExpiresAt) {
      params.set("google_token_expires_at", String(googleTokenExpiresAt));
    }
  }
  return NextResponse.redirect(`http://localhost:${portNum}/callback?${params}`);
}
