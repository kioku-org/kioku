import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Privacy Policy — Kioku",
  description: "How Kioku collects, uses, and protects your data.",
};

const LAST_UPDATED = "July 3, 2026";

export default function PrivacyPage() {
  return (
    <main className="min-h-screen bg-background text-foreground">
      <div className="mx-auto max-w-3xl px-6 py-16">
        <h1 className="text-3xl font-bold tracking-tight">Privacy Policy</h1>
        <p className="mt-2 text-sm text-muted-foreground">Last updated: {LAST_UPDATED}</p>

        <div className="mt-10 space-y-8 text-sm leading-7 text-foreground [&_h2]:mt-10 [&_h2]:text-lg [&_h2]:font-semibold [&_h2]:tracking-tight [&_p]:mt-3 [&_ul]:mt-3 [&_ul]:list-disc [&_ul]:pl-6 [&_li]:mt-1">
          <p>
            This policy covers the hosted Kioku service at <code>dashboard.kioku.chat</code> and
            related domains (the &quot;Service&quot;), operated by the Kioku project. Kioku is
            also open source and self-hostable — if you&apos;re using a self-hosted instance run
            by someone else, that operator controls your data and this policy does not apply to
            them.
          </p>

          <section>
            <h2>Information we collect</h2>
            <p>
              <strong>Account information.</strong> When you sign in with Google or GitHub, we
              receive your name and email address from that provider. We don&apos;t receive your
              password.
            </p>
            <p>
              <strong>Meeting data.</strong> When you invite the Kioku bot to a meeting (Google
              Meet, Microsoft Teams, or Zoom), it joins the call, records audio, and generates a
              transcript. We store the transcript, participant names as they appear in the
              meeting, and any summary generated from it.
            </p>
            <p>
              <strong>Uploaded documents.</strong> Files you upload (currently PDF) are stored,
              and their text is extracted and indexed so it can be searched later.
            </p>
            <p>
              <strong>Google Calendar (optional).</strong> If you connect Google Calendar (via{" "}
              <code>kioku cal</code> or the dashboard), we request read-only access
              (<code>calendar.readonly</code>) solely to display your upcoming events. We do not
              create, edit, or delete anything in your calendar, and we do not access any other
              Google data beyond your basic profile and this calendar scope.
            </p>
            <p>
              <strong>Usage data.</strong> We keep basic operational logs (e.g. API requests,
              errors) to run and secure the Service.
            </p>
          </section>

          <section>
            <h2>How we use this information</h2>
            <ul>
              <li>To provide the Service: authenticating you, running meeting bots, transcribing and summarizing meetings, indexing documents, and answering searches over your workspace&apos;s knowledge base.</li>
              <li>To display your Google Calendar events, if you&apos;ve connected it.</li>
              <li>To operate the MCP (Model Context Protocol) integration, which lets an AI client you configure (e.g. Claude, Cursor) query your own workspace&apos;s data on your behalf, using your own credentials.</li>
              <li>To maintain, secure, and improve the Service.</li>
            </ul>
            <p>
              Meeting transcripts, summaries, and documents are processed — including by AI
              models, which may be self-hosted or provided by a third party — in order to power
              search and summarization features. We do not sell your data to third parties or use
              it to train models outside of providing the Service to you.
            </p>
          </section>

          <section>
            <h2>Sharing within your workspace</h2>
            <p>
              Kioku organizes data into workspaces. Meetings, documents, and search results within
              a workspace are visible to every member of that workspace — there is currently no
              per-uploader privacy boundary within a shared workspace. A Free-tier workspace is
              limited to a single member; Pro and Teams workspaces can have multiple members who
              share this data by design.
            </p>
          </section>

          <section>
            <h2>Third parties</h2>
            <p>
              We use third-party infrastructure to provide the Service, including Google and
              GitHub (for sign-in), the Google Calendar API (if you connect it), and meeting
              platforms (Google Meet, Microsoft Teams, Zoom) to join and record meetings you
              invite our bot to. These providers process data as necessary to perform their part
              of the Service, under their own privacy terms.
            </p>
          </section>

          <section>
            <h2>Data retention and deletion</h2>
            <p>
              You can delete individual documents (<code>kioku docs --delete</code>) and revoke
              API keys or Calendar access at any time. Deleting your account removes your
              workspace&apos;s stored data, subject to reasonable operational backups, which are
              purged on a rolling basis.
            </p>
          </section>

          <section>
            <h2>Security</h2>
            <p>
              We use industry-standard measures (encrypted transport, hashed/encrypted
              credentials, scoped access tokens) to protect your data, but no online service can
              guarantee absolute security.
            </p>
          </section>

          <section>
            <h2>Children&apos;s privacy</h2>
            <p>
              The Service is not directed at children under 13, and we do not knowingly collect
              data from them.
            </p>
          </section>

          <section>
            <h2>Changes to this policy</h2>
            <p>
              We may update this policy from time to time. Material changes will be reflected by
              updating the &quot;Last updated&quot; date above.
            </p>
          </section>

          <section>
            <h2>Contact</h2>
            <p>
              Questions about this policy or your data? Email{" "}
              <a className="underline" href="mailto:hello@kioku.chat">
                hello@kioku.chat
              </a>
              .
            </p>
          </section>
        </div>
      </div>
    </main>
  );
}
