import { getServerSession } from "next-auth";
import { redirect } from "next/navigation";
import { authOptions } from "@/app/api/auth/[...nextauth]/route";

interface PageProps {
  searchParams: Promise<{ port?: string; state?: string; provider?: string }>;
}

export default async function CliAuthPage({ searchParams }: PageProps) {
  const { port, state, provider = "google" } = await searchParams;

  // ── Validate params ──────────────────────────────────────────────────────
  const portNum = parseInt(port ?? "", 10);
  if (!port || isNaN(portNum) || portNum < 1024 || portNum > 65535) {
    return <ErrorPage message="Invalid or missing port parameter." />;
  }
  if (!state || state.length < 8) {
    return <ErrorPage message="Invalid or missing state parameter." />;
  }

  // ── Check session ────────────────────────────────────────────────────────
  const session = await getServerSession(authOptions);

  if (!session?.user?.email) {
    // Not signed in — send directly to the OAuth provider
    const callbackUrl = `/cli-auth?port=${port}&state=${encodeURIComponent(state)}`;
    const providerPath = provider === "github" ? "github" : "google";
    redirect(`/api/auth/signin/${providerPath}?callbackUrl=${encodeURIComponent(callbackUrl)}`);
  }

  // ── Provision Hivemind JWT ────────────────────────────────────────────────
  const email = session.user.email;
  const name = session.user.name ?? email.split("@")[0];

  const hivemindUrl = process.env.HIVEMIND_INTERNAL_URL ?? "http://localhost:9100";
  const internalSecret = process.env.INTERNAL_API_SECRET;

  if (!internalSecret) {
    return <ErrorPage message="Server configuration error: missing INTERNAL_API_SECRET." />;
  }

  let hivemindData: {
    token: string;
    user_id: string;
    email: string;
    name: string;
    company_id: string;
    role: string;
  };

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
      return <ErrorPage message={`Backend provision failed (${r.status}). Please retry.`} />;
    }
    hivemindData = await r.json();
  } catch {
    return <ErrorPage message="Could not reach the Kioku backend. Please retry." />;
  }

  // ── Redirect to CLI callback ──────────────────────────────────────────────
  const params = new URLSearchParams({
    token: hivemindData.token,
    state,
    user_id: hivemindData.user_id,
    email: hivemindData.email,
    name: hivemindData.name,
    company_id: hivemindData.company_id,
    role: hivemindData.role,
  });

  const callbackUrl = `http://localhost:${portNum}/callback?${params.toString()}`;

  // Use a client-side JS redirect for http://localhost — the most reliable
  // way to go from an HTTPS page to a localhost URL without browser friction.
  return <RedirectPage url={callbackUrl} name={name} email={email} />;
}

// ── Sub-components ────────────────────────────────────────────────────────────

function RedirectPage({ url, name, email }: { url: string; name: string; email: string }) {
  const display = name || email;
  return (
    <html lang="en">
      <head>
        <title>kioku — signing in</title>
        {/* eslint-disable-next-line @next/next/no-sync-scripts */}
        <script
          dangerouslySetInnerHTML={{
            __html: `window.location.replace(${JSON.stringify(url)})`,
          }}
        />
      </head>
      <body
        style={{
          margin: 0,
          background: "#0c0c0c",
          color: "#fafaf9",
          fontFamily: "system-ui, sans-serif",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          minHeight: "100vh",
          textAlign: "center",
        }}
      >
        <div>
          <div style={{ fontSize: "3rem", marginBottom: "1rem" }}>𓄿</div>
          <h1 style={{ fontSize: "1.4rem", fontWeight: 700, marginBottom: "0.5rem" }}>
            Signed in as {display}
          </h1>
          <p style={{ color: "#737373", fontSize: "0.9rem" }}>
            Redirecting back to your terminal…
          </p>
          <p style={{ color: "#525252", fontSize: "0.8rem", marginTop: "1rem" }}>
            If nothing happens,{" "}
            <a href={url} style={{ color: "#fafaf9" }}>
              click here
            </a>
            .
          </p>
        </div>
      </body>
    </html>
  );
}

function ErrorPage({ message }: { message: string }) {
  return (
    <html lang="en">
      <head>
        <title>kioku — sign-in error</title>
      </head>
      <body
        style={{
          margin: 0,
          background: "#0c0c0c",
          color: "#fafaf9",
          fontFamily: "system-ui, sans-serif",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          minHeight: "100vh",
          textAlign: "center",
        }}
      >
        <div>
          <div style={{ fontSize: "3rem", marginBottom: "1rem" }}>𓄿</div>
          <h1 style={{ fontSize: "1.4rem", fontWeight: 700, marginBottom: "0.5rem" }}>
            Sign-in error
          </h1>
          <p style={{ color: "#737373", fontSize: "0.9rem" }}>{message}</p>
          <p style={{ color: "#525252", fontSize: "0.8rem", marginTop: "1rem" }}>
            Close this tab and run <code>kioku signin</code> again.
          </p>
        </div>
      </body>
    </html>
  );
}
