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

  if (!session?.user?.email) {
    // Not signed in — send to the OAuth provider, come back here after
    const self = `/cli-auth?port=${port}&state=${encodeURIComponent(state)}&provider=${provider}`;
    const providerPath = provider === "github" ? "github" : "google";
    return NextResponse.redirect(
      new URL(`/api/auth/signin/${providerPath}?callbackUrl=${encodeURIComponent(self)}`, request.url)
    );
  }

  // Signed in — provision (or re-use) the Hivemind JWT
  const hivemindToken: string | undefined = (session as any).hivemindToken;
  const email = session.user.email;
  const name = session.user.name ?? email.split("@")[0];

  // If the session already carries a hivemindToken, use it directly — but
  // only if it's actually a post-rename token. Sessions cached before the
  // company->workspace rename carry a JWT with no `workspace_id` claim;
  // trusting it here would silently redirect the CLI with an empty
  // workspace_id instead of failing loudly. Falling through re-provisions,
  // which mints a fresh, current-shape token.
  if (hivemindToken) {
    try {
      const payload = JSON.parse(
        Buffer.from(hivemindToken.split(".")[1], "base64url").toString()
      );
      if (payload.user_id && payload.workspace_id) {
        const params = new URLSearchParams({
          token: hivemindToken,
          state: state!,
          user_id: payload.user_id,
          email: payload.email ?? email,
          name: payload.name ?? name,
          workspace_id: payload.workspace_id,
          role: payload.role ?? "member",
        });
        return NextResponse.redirect(`http://localhost:${portNum}/callback?${params}`);
      }
      // Missing user_id/workspace_id — stale pre-rename token, fall through.
    } catch {
      // Malformed token — fall through to re-provision
    }
  }

  // No cached token: call provision endpoint
  const hivemindUrl = process.env.HIVEMIND_INTERNAL_URL ?? "http://localhost:9100";
  const internalSecret = process.env.INTERNAL_API_SECRET;
  if (!internalSecret) {
    return new NextResponse("Server configuration error: missing INTERNAL_API_SECRET.", { status: 500 });
  }

  try {
    const r = await fetch(`${hivemindUrl}/internal/provision`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Internal-Secret": internalSecret,
      },
      body: JSON.stringify({ email, name }),
    });
    if (!r.ok) {
      return new NextResponse(`Backend provision failed (${r.status}).`, { status: 502 });
    }
    const data = await r.json();
    const params = new URLSearchParams({
      token: data.token,
      state: state!,
      user_id: data.user_id,
      email: data.email,
      name: data.name,
      workspace_id: data.workspace_id,
      role: data.role,
    });
    return NextResponse.redirect(`http://localhost:${portNum}/callback?${params}`);
  } catch {
    return new NextResponse("Could not reach the Kioku backend.", { status: 502 });
  }
}
