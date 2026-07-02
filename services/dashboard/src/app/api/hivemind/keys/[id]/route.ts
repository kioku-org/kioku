import { NextResponse } from "next/server";
import { getServerSession } from "next-auth";
import { authOptions } from "@/app/api/auth/[...nextauth]/route";

function hivemindUrl() {
  return process.env.HIVEMIND_INTERNAL_URL || "http://localhost:9100";
}

export async function DELETE(_req: Request, { params }: { params: Promise<{ id: string }> }) {
  const session = await getServerSession(authOptions);
  const token = (session as any)?.hivemindToken;
  if (!token) {
    return NextResponse.json({ error: "Not provisioned with Hivemind" }, { status: 401 });
  }

  const { id } = await params;
  const res = await fetch(`${hivemindUrl()}/workspace/auth-keys/${id}`, {
    method: "DELETE",
    headers: { Authorization: `Bearer ${token}` },
  });

  if (res.status === 200) return NextResponse.json({});
  const data = await res.json().catch(() => ({}));
  return NextResponse.json(data, { status: res.status });
}
