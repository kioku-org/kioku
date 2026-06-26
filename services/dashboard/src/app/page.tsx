import Link from "next/link";

export default function Home() {
  return (
    <main className="flex min-h-screen flex-col items-center justify-center p-8">
      <div className="max-w-2xl text-center space-y-6">
        <h1 className="text-4xl font-bold tracking-tight">
          Kioku Dashboard
        </h1>
        <p className="text-lg text-muted-foreground">
          Manage your meetings, transcripts, and knowledge base.
        </p>
        <div className="flex gap-4 justify-center pt-4">
          <Link
            href="/meetings"
            className="inline-flex items-center justify-center rounded-md bg-primary px-6 py-3 text-sm font-medium text-primary-foreground hover:bg-primary/90 transition-colors"
          >
            View Meetings
          </Link>
          <Link
            href="/settings"
            className="inline-flex items-center justify-center rounded-md border border-input bg-background px-6 py-3 text-sm font-medium hover:bg-accent hover:text-accent-foreground transition-colors"
          >
            Settings
          </Link>
        </div>
      </div>
    </main>
  );
}
