import type { Metadata } from "next";
import Link from "next/link";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import { FadeIn, BlurFade, TextGradient } from "@/components/animations";
import { getSiteOrigin } from "@/lib/site-url";

const securityDescription =
  "Cerul security — how we protect your data. AES-256 encryption, no model training on customer data, SOC 2 in progress, full data control. Contact security@cerul.ai.";

const siteOrigin = getSiteOrigin();

export const metadata: Metadata = {
  title: "Security",
  description: securityDescription,
  alternates: {
    canonical: "/security",
  },
  openGraph: {
    title: "Security at Cerul",
    description: securityDescription,
    url: `${siteOrigin}/security`,
    siteName: "Cerul",
    type: "website",
  },
  twitter: {
    card: "summary_large_image",
    title: "Security at Cerul",
    description: securityDescription,
  },
};

const dataWeCollect = [
  { item: "Video files (mp4 / mov / webm)", note: "For indexing only" },
  { item: "Audio extracted from video", note: "For transcription" },
  { item: "Keyframes (image samples)", note: "For visual embeddings" },
  { item: "Transcripts produced by Whisper", note: "Search index" },
  { item: "Metadata: title, duration, source URL, timestamps", note: "Search index" },
  { item: "ACL: who can access each video in the source platform", note: "Permission enforcement" },
];

const dataWeNeverTouch = [
  "Slack DMs or message content",
  "Slack channels we have not been added to",
  "Zoom meetings that were not cloud-recorded",
  "Account passwords (OAuth never exposes them)",
  "Any data outside the OAuth scopes you granted",
  "Any data from workspaces other than the one connected",
];

const retentionRules = [
  { key: "Raw video files", value: "Deleted 7 days after indexing (configurable: immediate, or kept for re-embed)" },
  { key: "Transcripts", value: "Retained while workspace is active" },
  { key: "Embeddings", value: "Retained while workspace is active" },
  { key: "Query logs", value: "90 days, then aggregated / anonymized" },
  { key: "Backups", value: "Encrypted, cross-region, 30-day rotation" },
  { key: "On workspace deletion", value: "All data purged within 7 days, no recovery" },
  { key: "On OAuth revocation", value: "Token invalidated within seconds; index becomes read-only" },
];

const compliance = [
  { status: "✅", item: "GDPR-compliant DPA available for any customer" },
  { status: "✅", item: "CCPA-compliant" },
  { status: "🚧", item: "SOC 2 Type II — audit in progress" },
  { status: "🚧", item: "ISO 27001 — planned Year 2" },
  { status: "🚧", item: "HIPAA + BAA — available on Enterprise with healthcare addendum" },
];

const subprocessors = [
  { name: "AWS", purpose: "Hosting, storage, encryption (KMS)" },
  { name: "Cloudflare", purpose: "CDN, DDoS protection, edge" },
  { name: "Neon", purpose: "Managed PostgreSQL" },
  { name: "Stripe", purpose: "Billing" },
  { name: "Resend", purpose: "Transactional email" },
  { name: "PostHog", purpose: "Product analytics (workspace-aggregated; no content)" },
  { name: "Slack / Zoom / Loom", purpose: "Connector APIs (per-customer OAuth)" },
];

const customerControls = [
  {
    title: "Revoke any connector",
    description:
      "From your source platform's admin console. We stop fetching new content within seconds; existing index becomes read-only.",
  },
  {
    title: "Export your data",
    description: "All transcripts, metadata, and audit logs as JSON / CSV at any time.",
  },
  {
    title: "Delete specific videos",
    description: "From the Cerul Flow admin UI, anytime.",
  },
  {
    title: "Delete your workspace",
    description: "Self-serve, 7-day grace period, then permanent purge of all data.",
  },
  {
    title: "Request a security report",
    description: "Pen test summary, SOC 2 progress, security questionnaires (SIG, CAIQ) under NDA.",
  },
];

const webPageJsonLd = {
  "@context": "https://schema.org",
  "@type": "WebPage",
  name: "Security at Cerul",
  description: securityDescription,
  url: `${siteOrigin}/security`,
  isPartOf: {
    "@type": "WebSite",
    name: "Cerul",
    url: siteOrigin,
  },
};

export default function SecurityPage() {
  return (
    <>
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(webPageJsonLd) }}
      />
      <div className="soft-theme overflow-x-clip">
        <div className="mx-auto flex min-h-screen max-w-[1400px] flex-col px-4 pb-10 pt-4 sm:px-6 lg:px-8">
          <SiteHeader currentPath="/security" />

          <main className="flex flex-1 flex-col">
            {/* Hero */}
            <section className="relative py-16 lg:py-24">
              <div className="mx-auto max-w-4xl text-center">
                <BlurFade delay={100}>
                  <span className="eyebrow inline-flex items-center gap-2 rounded-full border border-[var(--border-brand)] bg-[var(--brand-subtle)] px-4 py-1.5 text-xs font-semibold uppercase tracking-wider text-[var(--brand-bright)]">
                    <span className="h-1.5 w-1.5 rounded-full bg-[var(--brand)] animate-pulse" />
                    Security
                  </span>
                </BlurFade>

                <BlurFade delay={200}>
                  <h1 className="mt-8 text-4xl font-bold leading-[1.1] tracking-tight text-[var(--foreground)] sm:text-5xl lg:text-6xl">
                    Security at <TextGradient animate>Cerul</TextGradient>
                  </h1>
                </BlurFade>

                <BlurFade delay={300}>
                  <p className="mx-auto mt-6 max-w-2xl text-lg leading-relaxed text-[var(--foreground-secondary)]">
                    Cerul holds your team&apos;s video archive. We treat that responsibility seriously.
                    This page documents how — encryption, data handling, compliance, and the
                    controls you keep.
                  </p>
                </BlurFade>

                <BlurFade delay={400}>
                  <p className="mt-6 text-sm text-[var(--foreground-tertiary)]">
                    Last updated 2026-05-17 · Contact{" "}
                    <a
                      href="mailto:security@cerul.ai"
                      className="text-[var(--brand-bright)] hover:text-[var(--foreground)]"
                    >
                      security@cerul.ai
                    </a>
                  </p>
                </BlurFade>
              </div>
            </section>

            {/* No-training commitment — flagship section, most prominent */}
            <section className="py-8 lg:py-12">
              <FadeIn>
                <div className="relative overflow-hidden rounded-[32px] border-2 border-[var(--border-brand)] bg-gradient-to-br from-[var(--brand-subtle)] via-[var(--surface)] to-[var(--background-elevated)] p-10 lg:p-14">
                  <div className="absolute -right-24 -top-24 h-56 w-56 rounded-full bg-[var(--brand)] opacity-15 blur-3xl" />
                  <div className="relative">
                    <span className="eyebrow text-[var(--brand-bright)]">Our foundational promise</span>
                    <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl lg:text-5xl">
                      Your data never trains any model.
                    </h2>
                    <div className="mt-6 grid gap-4 text-base leading-relaxed text-[var(--foreground-secondary)] lg:grid-cols-2">
                      <ul className="space-y-3">
                        <li className="flex items-start gap-3">
                          <svg className="mt-1 h-5 w-5 flex-shrink-0 text-[var(--brand-bright)]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5}><path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" /></svg>
                          <span>We do not train any Cerul model on customer videos, transcripts, embeddings, or queries.</span>
                        </li>
                        <li className="flex items-start gap-3">
                          <svg className="mt-1 h-5 w-5 flex-shrink-0 text-[var(--brand-bright)]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5}><path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" /></svg>
                          <span>We do not share customer data with third-party model providers for training.</span>
                        </li>
                      </ul>
                      <ul className="space-y-3">
                        <li className="flex items-start gap-3">
                          <svg className="mt-1 h-5 w-5 flex-shrink-0 text-[var(--brand-bright)]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5}><path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" /></svg>
                          <span>We do not use one customer&apos;s data to improve another customer&apos;s search results.</span>
                        </li>
                        <li className="flex items-start gap-3">
                          <svg className="mt-1 h-5 w-5 flex-shrink-0 text-[var(--brand-bright)]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5}><path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" /></svg>
                          <span>This commitment is binding — written into our DPA and Terms of Service.</span>
                        </li>
                      </ul>
                    </div>
                    <p className="mt-6 text-sm text-[var(--foreground-tertiary)]">
                      If we ever change this policy, we will give you 90 days written notice and the right to opt out and export all of your data.
                    </p>
                  </div>
                </div>
              </FadeIn>
            </section>

            {/* What we collect / never touch */}
            <section className="py-16 lg:py-20">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  Data we hold
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                  What we collect — and what we never touch
                </h2>
              </FadeIn>

              <div className="mt-12 grid gap-6 lg:grid-cols-2">
                <FadeIn delay={100}>
                  <div className="h-full rounded-3xl border border-[var(--border)] bg-[var(--surface)] p-8">
                    <h3 className="text-lg font-semibold text-[var(--foreground)]">
                      What we collect via your connectors
                    </h3>
                    <ul className="mt-6 space-y-4">
                      {dataWeCollect.map((entry) => (
                        <li key={entry.item} className="flex items-start gap-3">
                          <svg className="mt-1 h-4 w-4 flex-shrink-0 text-[var(--success)]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5}>
                            <path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" />
                          </svg>
                          <div>
                            <p className="text-sm text-[var(--foreground)]">{entry.item}</p>
                            <p className="mt-0.5 text-xs text-[var(--foreground-tertiary)]">{entry.note}</p>
                          </div>
                        </li>
                      ))}
                    </ul>
                  </div>
                </FadeIn>

                <FadeIn delay={200}>
                  <div className="h-full rounded-3xl border border-[var(--border)] bg-[var(--surface)] p-8">
                    <h3 className="text-lg font-semibold text-[var(--foreground)]">
                      What we never touch
                    </h3>
                    <ul className="mt-6 space-y-3">
                      {dataWeNeverTouch.map((item) => (
                        <li key={item} className="flex items-start gap-3 text-sm text-[var(--foreground-secondary)]">
                          <svg className="mt-1 h-4 w-4 flex-shrink-0 text-[var(--error)]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5}>
                            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                          </svg>
                          <span>{item}</span>
                        </li>
                      ))}
                    </ul>
                  </div>
                </FadeIn>
              </div>
            </section>

            {/* Encryption */}
            <section className="py-16 lg:py-20">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  Encryption
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                  Encrypted everywhere
                </h2>
              </FadeIn>

              <div className="mt-12 grid gap-5 md:grid-cols-2 lg:grid-cols-4">
                {[
                  { label: "In transit", value: "TLS 1.3", note: "All API and bot traffic" },
                  { label: "At rest", value: "AES-256", note: "Per-workspace encryption keys" },
                  { label: "Key storage", value: "Cloud KMS", note: "AWS KMS / Google Cloud KMS — keys never in code or DB" },
                  { label: "Backups", value: "Encrypted", note: "Separate keys, separate region, 30-day rotation" },
                ].map((box, i) => (
                  <FadeIn key={box.label} delay={i * 80}>
                    <div className="h-full rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-6">
                      <p className="text-xs font-semibold uppercase tracking-wider text-[var(--brand-bright)]">
                        {box.label}
                      </p>
                      <p className="mt-3 text-2xl font-bold text-[var(--foreground)]">{box.value}</p>
                      <p className="mt-2 text-xs text-[var(--foreground-secondary)]">{box.note}</p>
                    </div>
                  </FadeIn>
                ))}
              </div>

              <FadeIn delay={300}>
                <p className="mt-8 text-center text-sm text-[var(--foreground-secondary)]">
                  Enterprise customers can bring their own keys (BYOK) via AWS KMS, Azure Key Vault, or GCP KMS.{" "}
                  <Link href="/enterprise" className="text-[var(--brand-bright)] hover:text-[var(--foreground)]">
                    Contact sales
                  </Link>
                  .
                </p>
              </FadeIn>
            </section>

            {/* Data retention */}
            <section className="py-16 lg:py-20">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  Retention
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                  We hold only what is needed, only as long as needed
                </h2>
              </FadeIn>

              <FadeIn delay={100}>
                <div className="mt-12 overflow-hidden rounded-3xl border border-[var(--border)] bg-[var(--surface)]">
                  <div className="overflow-x-auto">
                    <table className="w-full">
                      <thead>
                        <tr className="border-b border-[var(--border)] bg-[var(--background-elevated)]">
                          <th className="px-6 py-4 text-left text-sm font-semibold text-[var(--foreground)]">
                            Data class
                          </th>
                          <th className="px-6 py-4 text-left text-sm font-semibold text-[var(--foreground)]">
                            Retention
                          </th>
                        </tr>
                      </thead>
                      <tbody>
                        {retentionRules.map((row, index) => (
                          <tr
                            key={row.key}
                            className={index !== retentionRules.length - 1 ? "border-b border-[var(--border)]" : ""}
                          >
                            <td className="px-6 py-4 text-sm font-medium text-[var(--foreground)]">
                              {row.key}
                            </td>
                            <td className="px-6 py-4 text-sm text-[var(--foreground-secondary)]">
                              {row.value}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>
              </FadeIn>
            </section>

            {/* Access controls */}
            <section className="py-16 lg:py-20">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  Access
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                  Who can access your data
                </h2>
              </FadeIn>

              <div className="mt-12 grid gap-6 md:grid-cols-3">
                {[
                  {
                    title: "Customer admins",
                    body: "Full access within their workspace via product UI.",
                  },
                  {
                    title: "Customer users",
                    body: "ACL-filtered access. Can only see what their source-platform permissions allow.",
                  },
                  {
                    title: "Cerul employees",
                    body: "No routine access. Support requires a customer-approved, time-limited grant (max 24h), with every action logged.",
                  },
                ].map((card, i) => (
                  <FadeIn key={card.title} delay={i * 100}>
                    <div className="h-full rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-7">
                      <h3 className="text-base font-semibold text-[var(--foreground)]">{card.title}</h3>
                      <p className="mt-3 text-sm leading-relaxed text-[var(--foreground-secondary)]">{card.body}</p>
                    </div>
                  </FadeIn>
                ))}
              </div>

              <FadeIn delay={300}>
                <p className="mt-8 text-center text-sm text-[var(--foreground-secondary)]">
                  Every access event — customer or Cerul — is recorded in an audit log visible to customer admins.
                </p>
              </FadeIn>
            </section>

            {/* Compliance */}
            <section className="py-16 lg:py-20">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  Compliance
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                  Built for procurement
                </h2>
              </FadeIn>

              <FadeIn delay={100}>
                <ul className="mx-auto mt-12 max-w-2xl space-y-3">
                  {compliance.map((row) => (
                    <li
                      key={row.item}
                      className="flex items-center gap-4 rounded-2xl border border-[var(--border)] bg-[var(--surface)] px-5 py-4"
                    >
                      <span className="text-xl">{row.status}</span>
                      <span className="text-sm text-[var(--foreground)]">{row.item}</span>
                    </li>
                  ))}
                </ul>
              </FadeIn>

              <FadeIn delay={200}>
                <p className="mt-8 text-center text-sm text-[var(--foreground-secondary)]">
                  Standard questionnaires we can complete: SIG Lite, SIG Core, CAIQ, VSA.{" "}
                  <a href="mailto:security@cerul.ai" className="text-[var(--brand-bright)] hover:text-[var(--foreground)]">
                    Request our trust portal
                  </a>
                  .
                </p>
              </FadeIn>
            </section>

            {/* Infrastructure */}
            <section className="py-16 lg:py-20">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  Infrastructure
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                  Production-grade by default
                </h2>
              </FadeIn>

              <div className="mt-12 grid gap-5 md:grid-cols-2 lg:grid-cols-3">
                {[
                  { title: "Hosting", body: "AWS US-East (Virginia) primary, US-West (Oregon) backup. EU (Frankfurt) available on Enterprise." },
                  { title: "Isolation", body: "Per-workspace data isolation at storage and query layers. Vector indexes are stored per-tenant." },
                  { title: "Monitoring", body: "24/7 automated monitoring. On-call engineer for sev-1 incidents." },
                  { title: "Backups", body: "Daily encrypted, retained 30 days, cross-region replication." },
                  { title: "DDoS", body: "Cloudflare in front of all public endpoints." },
                  { title: "Network", body: "Private subnets, security groups, no public databases." },
                ].map((card, i) => (
                  <FadeIn key={card.title} delay={i * 60}>
                    <div className="h-full rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-6">
                      <h3 className="text-sm font-semibold text-[var(--foreground)]">{card.title}</h3>
                      <p className="mt-2 text-xs leading-relaxed text-[var(--foreground-secondary)]">{card.body}</p>
                    </div>
                  </FadeIn>
                ))}
              </div>
            </section>

            {/* Customer controls */}
            <section className="py-16 lg:py-20">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  Your controls
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                  You stay in control — always
                </h2>
              </FadeIn>

              <div className="mt-12 grid gap-5 md:grid-cols-2 lg:grid-cols-3">
                {customerControls.map((control, i) => (
                  <FadeIn key={control.title} delay={i * 80}>
                    <div className="h-full rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-6">
                      <h3 className="text-base font-semibold text-[var(--foreground)]">
                        {control.title}
                      </h3>
                      <p className="mt-3 text-sm leading-relaxed text-[var(--foreground-secondary)]">
                        {control.description}
                      </p>
                    </div>
                  </FadeIn>
                ))}
              </div>
            </section>

            {/* Subprocessors */}
            <section className="py-16 lg:py-20">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  Subprocessors
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                  Third parties we use to operate Cerul
                </h2>
                <p className="mx-auto mt-4 max-w-2xl text-[var(--foreground-secondary)]">
                  We disclose every third-party service that may process customer data. Email{" "}
                  <a href="mailto:security@cerul.ai" className="text-[var(--brand-bright)] hover:text-[var(--foreground)]">
                    security@cerul.ai
                  </a>{" "}
                  to subscribe to subprocessor change notifications.
                </p>
              </FadeIn>

              <FadeIn delay={100}>
                <div className="mt-12 overflow-hidden rounded-3xl border border-[var(--border)] bg-[var(--surface)]">
                  <div className="overflow-x-auto">
                    <table className="w-full">
                      <thead>
                        <tr className="border-b border-[var(--border)] bg-[var(--background-elevated)]">
                          <th className="px-6 py-4 text-left text-sm font-semibold text-[var(--foreground)]">Vendor</th>
                          <th className="px-6 py-4 text-left text-sm font-semibold text-[var(--foreground)]">Purpose</th>
                        </tr>
                      </thead>
                      <tbody>
                        {subprocessors.map((row, index) => (
                          <tr
                            key={row.name}
                            className={index !== subprocessors.length - 1 ? "border-b border-[var(--border)]" : ""}
                          >
                            <td className="px-6 py-4 text-sm font-medium text-[var(--foreground)]">{row.name}</td>
                            <td className="px-6 py-4 text-sm text-[var(--foreground-secondary)]">{row.purpose}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>
              </FadeIn>
            </section>

            {/* Incident response */}
            <section className="py-16 lg:py-20">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  Incident response
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                  If something goes wrong
                </h2>
              </FadeIn>

              <div className="mt-12 grid gap-6 lg:grid-cols-2">
                <FadeIn delay={100}>
                  <div className="h-full rounded-3xl border border-[var(--border)] bg-[var(--surface)] p-8">
                    <h3 className="text-lg font-semibold text-[var(--foreground)]">
                      Report a security issue
                    </h3>
                    <p className="mt-3 text-sm text-[var(--foreground-secondary)]">
                      Email{" "}
                      <a
                        href="mailto:security@cerul.ai"
                        className="text-[var(--brand-bright)] hover:text-[var(--foreground)]"
                      >
                        security@cerul.ai
                      </a>
                      . We respond within 24 hours.
                    </p>
                    <p className="mt-4 text-xs text-[var(--foreground-tertiary)]">
                      A formal bug bounty program is planned for Q3 2026 via HackerOne. In the
                      meantime, responsible disclosures are welcomed and acknowledged.
                    </p>
                  </div>
                </FadeIn>

                <FadeIn delay={200}>
                  <div className="h-full rounded-3xl border border-[var(--border)] bg-[var(--surface)] p-8">
                    <h3 className="text-lg font-semibold text-[var(--foreground)]">
                      If a breach affects your data
                    </h3>
                    <ul className="mt-4 space-y-3 text-sm text-[var(--foreground-secondary)]">
                      <li className="flex items-start gap-2">
                        <svg className="mt-1 h-4 w-4 flex-shrink-0 text-[var(--brand-bright)]" fill="currentColor" viewBox="0 0 8 8"><circle cx="4" cy="4" r="3" /></svg>
                        <span>You will be notified within 72 hours.</span>
                      </li>
                      <li className="flex items-start gap-2">
                        <svg className="mt-1 h-4 w-4 flex-shrink-0 text-[var(--brand-bright)]" fill="currentColor" viewBox="0 0 8 8"><circle cx="4" cy="4" r="3" /></svg>
                        <span>We will tell you what was accessed, when, by whom (if known), and what data was involved.</span>
                      </li>
                      <li className="flex items-start gap-2">
                        <svg className="mt-1 h-4 w-4 flex-shrink-0 text-[var(--brand-bright)]" fill="currentColor" viewBox="0 0 8 8"><circle cx="4" cy="4" r="3" /></svg>
                        <span>We will share remediation steps and timeline.</span>
                      </li>
                    </ul>
                  </div>
                </FadeIn>
              </div>
            </section>

            {/* CTA */}
            <section className="py-16 lg:py-24">
              <FadeIn>
                <div className="relative overflow-hidden rounded-[32px] border border-[var(--border)] bg-gradient-to-br from-[var(--surface)] via-[var(--background)] to-[var(--surface)] p-12 text-center lg:p-16">
                  <div className="absolute inset-0 bg-[radial-gradient(circle_at_center,rgba(34,211,238,0.08),transparent_55%)]" />
                  <div className="relative">
                    <h2 className="text-2xl font-bold tracking-tight text-[var(--foreground)] sm:text-3xl lg:text-4xl">
                      Questions, or running a security review?
                    </h2>
                    <p className="mx-auto mt-4 max-w-xl text-[var(--foreground-secondary)]">
                      We respond to security questionnaires, DPAs, and trust portal requests
                      within one business day.
                    </p>
                    <div className="mt-8 flex flex-wrap items-center justify-center gap-4">
                      <a
                        href="mailto:security@cerul.ai"
                        className="button-gradient"
                      >
                        security@cerul.ai
                      </a>
                      <Link href="/enterprise" className="button-secondary">
                        Talk to enterprise
                      </Link>
                    </div>
                  </div>
                </div>
              </FadeIn>
            </section>
          </main>

          <SiteFooter />
        </div>
      </div>
    </>
  );
}
