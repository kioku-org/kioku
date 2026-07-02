import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Terms of Service — Kioku",
  description: "The terms governing use of the Kioku service.",
};

const LAST_UPDATED = "July 3, 2026";

export default function TermsPage() {
  return (
    <main className="min-h-screen bg-background text-foreground">
      <div className="mx-auto max-w-3xl px-6 py-16">
        <h1 className="text-3xl font-bold tracking-tight">Terms of Service</h1>
        <p className="mt-2 text-sm text-muted-foreground">Last updated: {LAST_UPDATED}</p>

        <div className="mt-10 space-y-8 text-sm leading-7 text-foreground [&_h2]:mt-10 [&_h2]:text-lg [&_h2]:font-semibold [&_h2]:tracking-tight [&_p]:mt-3 [&_ul]:mt-3 [&_ul]:list-disc [&_ul]:pl-6 [&_li]:mt-1">
          <p>
            These terms govern your use of the hosted Kioku service at{" "}
            <code>dashboard.kioku.chat</code> and related domains (the &quot;Service&quot;). By
            using the Service, you agree to these terms. If you don&apos;t agree, please don&apos;t
            use the Service.
          </p>

          <section>
            <h2>The Service</h2>
            <p>
              Kioku lets you record and transcribe meetings, upload and search documents, and
              connect Google Calendar and MCP-compatible AI clients to a shared knowledge base
              organized into workspaces. Kioku is also open source (MIT-licensed) and
              self-hostable; these terms apply only to the hosted Service we operate, not to
              instances run by others.
            </p>
          </section>

          <section>
            <h2>Your account</h2>
            <p>
              You&apos;re responsible for keeping your sign-in credentials, API keys, and CLI
              tokens confidential, and for all activity that happens under your account. Tell us
              promptly at{" "}
              <a className="underline" href="mailto:hello@kioku.chat">
                hello@kioku.chat
              </a>{" "}
              if you believe your account or a key has been compromised.
            </p>
          </section>

          <section>
            <h2>Recording meetings — your responsibility</h2>
            <p>
              Inviting the Kioku bot into a meeting causes it to record and transcribe that
              meeting. Recording laws vary by jurisdiction — many require the consent of some or
              all participants before a call can be recorded. <strong>You are solely responsible
              for ensuring you have the legal right and any required consent to record a meeting
              before inviting the bot to it.</strong> Kioku is a tool; we don&apos;t verify consent
              on your behalf.
            </p>
          </section>

          <section>
            <h2>Acceptable use</h2>
            <p>You agree not to:</p>
            <ul>
              <li>Use the Service to violate any law, including recording-consent or data-protection laws.</li>
              <li>Attempt to disrupt, overload, or gain unauthorized access to the Service or other users&apos; workspaces.</li>
              <li>Upload content you don&apos;t have the right to upload, or that infringes someone else&apos;s rights.</li>
              <li>Use the Service to harass, defame, or harm others.</li>
            </ul>
          </section>

          <section>
            <h2>Workspaces and plans</h2>
            <p>
              Free-tier workspaces are limited to a single member. Pro and Teams plans unlock
              multi-member workspaces where meetings, documents, and search are shared among
              members, as described in our{" "}
              <a className="underline" href="/privacy">
                Privacy Policy
              </a>
              . Plan features and limits may change as the Service evolves.
            </p>
          </section>

          <section>
            <h2>Your content</h2>
            <p>
              You retain ownership of the meetings, documents, and other content you bring into
              the Service. You grant us the rights necessary to store, process, and index that
              content in order to provide the Service to you (e.g. transcription, embedding,
              search).
            </p>
          </section>

          <section>
            <h2>Termination</h2>
            <p>
              You may stop using the Service and delete your account at any time. We may suspend
              or terminate access to the Service for conduct that violates these terms or that we
              reasonably believe is harmful to the Service or other users.
            </p>
          </section>

          <section>
            <h2>Disclaimers and limitation of liability</h2>
            <p>
              The Service is provided &quot;as is,&quot; without warranties of any kind, express
              or implied. Kioku is under active development, and features (including this early
              build) may change or be interrupted. To the maximum extent permitted by law, Kioku
              and its contributors are not liable for indirect, incidental, or consequential
              damages arising from your use of the Service.
            </p>
          </section>

          <section>
            <h2>Changes to these terms</h2>
            <p>
              We may update these terms from time to time. Material changes will be reflected by
              updating the &quot;Last updated&quot; date above. Continuing to use the Service
              after changes take effect means you accept the updated terms.
            </p>
          </section>

          <section>
            <h2>Contact</h2>
            <p>
              Questions about these terms? Email{" "}
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
