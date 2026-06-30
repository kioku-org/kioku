import { NextRequest, NextResponse } from "next/server";
import { getServerSession } from "next-auth";
import { authOptions } from "@/app/api/auth/[...nextauth]/route";

function hivemindUrl() {
  return process.env.HIVEMIND_INTERNAL_URL || "http://localhost:9100";
}

async function getHivemindToken(): Promise<string | null> {
  const session = await getServerSession(authOptions);
  return (session as any)?.hivemindToken ?? null;
}

export async function GET() {
  const token = await getHivemindToken();
  if (!token) {
    return NextResponse.json({ error: "Not provisioned with Hivemind" }, { status: 401 });
  }

  const res = await fetch(`${hivemindUrl()}/company/auth-keys`, {
    headers: { Authorization: `Bearer ${token}` },
    cache: "no-store",
  });

  const data = await res.json();
  return NextResponse.json(data, { status: res.status });
}

export async function POST(request: NextRequest) {
  const token = await getHivemindToken();
  if (!token) {
    return NextResponse.json({ error: "Not provisioned with Hivemind" }, { status: 401 });
  }

  const body = await request.json();
  const res = await fetch(`${hivemindUrl()}/company/auth-keys`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ name: body.name || "cli-key" }),
  });

  const data = await res.json();
  return NextResponse.json(data, { status: res.status });
}
