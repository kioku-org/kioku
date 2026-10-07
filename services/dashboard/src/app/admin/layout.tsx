import { AdminGuard } from "@/components/admin/admin-guard";

// Admin entry pages must not remain in a shared proxy cache across deployments.
export const dynamic = "force-dynamic";

export default function AdminLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return <AdminGuard>{children}</AdminGuard>;
}
