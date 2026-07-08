import type { Metadata } from "next";
import Image from "next/image";
import Link from "next/link";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import { Features } from "@/components/features";
import { GoogleOneTap } from "@/components/google-one-tap";
import {
  FadeIn,
  BlurFade,
  TextGradient,
  AnimatedCounter,
} from "@/components/animations";
import { getSiteOrigin } from "@/lib/site-url";
import { homeOpenGraphImages, homeTwitterImages } from "@/lib/social-metadata";
import { getServerSession } from "@/lib/auth-server";
import { getAuthUiConfig } from "@/lib/auth-providers";

const homeDescription =
  "Cerul is the hosted video search API for AI agents. Use the Cloud API on cerul.ai, or download Cerul App for local-first desktop video and audio search.";

const siteOrigin = getSiteOrigin();

export const metadata: Metadata = {
  description: homeDescription,
  alternates: {
    canonical: "/",
  },
  openGraph: {
    title: "Cerul",
    description: homeDescription,
    url: siteOrigin,
    siteName: "Cerul",
    type: "website",
    images: homeOpenGraphImages,
  },
  twitter: {
    card: "summary_large_image",
    title: "Cerul",
    description: homeDescription,
    images: homeTwitterImages,
  },
};

const jsonLd = {
  "@context": "https://schema.org",
  "@graph": [
    {
      "@type": "Organization",
      name: "Cerul",
      url: siteOrigin,
      logo: `${siteOrigin}/logo.svg`,
      sameAs: [
        "https://github.com/cerul-ai/cerul",
        "https://x.com/cerul_hq",
        "https://discord.gg/qHDEMQB9vN",
      ],
    },
    {
      "@type": "WebSite",
      name: "Cerul",
      url: siteOrigin,
    },
    {
      "@type": "SoftwareApplication",
      name: "Cerul Cloud",
      applicationCategory: "DeveloperApplication",
      operatingSystem: "Web",
      description: homeDescription,
      url: siteOrigin,
      isAccessibleForFree: true,
      license: "https://github.com/cerul-ai/cerul/blob/main/LICENSE",
      codeRepository: "https://github.com/cerul-ai/cerul",
    },
    {
      "@type": "WebAPI",
      name: "Cerul Search API",
      description:
        "Where video becomes citable. The video search API for agents — meaning-based, grounded, timestamped.",
      documentation: `${siteOrigin}/docs`,
      url: siteOrigin,
      provider: {
        "@type": "Organization",
        name: "Cerul",
      },
    },
    {
      "@type": "SoftwareApplication",
      name: "Cerul App",
      applicationCategory: "UtilitiesApplication",
      operatingSystem: "macOS",
      description:
        "The local-first desktop app for searching personal video and audio libraries by meaning.",
      url: "https://app.cerul.ai",
      downloadUrl: "https://app.cerul.ai/download",
      isAccessibleForFree: true,
      publisher: {
        "@type": "Organization",
        name: "Cerul",
      },
    },
  ],
};

const features = [
  {
    icon: (
      <svg width="24" height="24" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
        <path strokeLinecap="round" strokeLinejoin="round" d="M2.036 12.322a1.012 1.012 0 010-.639C3.423 7.51 7.36 4.5 12 4.5c4.638 0 8.573 3.007 9.963 7.178.07.207.07.431 0 .639C20.577 16.49 16.64 19.5 12 19.5c-4.638 0-8.573-3.007-9.963-7.178z" />
        <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
      </svg>
    ),
    title: "Visual Search",
    description:
      "Slides, charts, demos, and whiteboard sketches become searchable context for your agent.",
    href: "/docs",
  },
  {
    icon: (
      <svg width="24" height="24" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
        <path strokeLinecap="round" strokeLinejoin="round" d="M12 18.75a6 6 0 006-6v-1.5m-6 7.5a6 6 0 01-6-6v-1.5m6 7.5v3.75m-3.75 0h7.5M12 15.75a3 3 0 01-3-3V4.5a3 3 0 116 0v8.25a3 3 0 01-3 3z" />
      </svg>
    ),
    title: "Speech + Transcript",
    description:
      "Every spoken word is indexed and aligned with the visual timeline, so you can search both.",
    href: "/docs",
  },
  {
    icon: (
      <svg width="24" height="24" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
        <path strokeLinecap="round" strokeLinejoin="round" d="M17.25 6.75L22.5 12l-5.25 5.25m-10.5 0L1.5 12l5.25-5.25m7.5-3l-4.5 16.5" />
      </svg>
    ),
    title: "One API Call",
    description:
      "A single endpoint returns grounded results with timestamps, relevance scores, and source links.",
    href: "/docs/api-reference",
  },
];

const stats = [
  { value: 99.9, suffix: "%", label: "Uptime SLA" },
  { value: 150, suffix: "ms", label: "Avg. latency" },
  { value: 10000, suffix: "+", label: "Videos indexed" },
  { value: 3, suffix: "x", label: "Faster search" },
];

export default async function HomePage() {
  const session = await getServerSession();
  const { googleOneTapClientId } = getAuthUiConfig();
  const showOneTap = !session && googleOneTapClientId;

  return (
    <>
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd) }}
      />
      {showOneTap && <GoogleOneTap clientId={googleOneTapClientId} />}
      <div className="soft-theme overflow-x-clip">
        <div className="relative z-10 mx-auto flex min-h-screen max-w-[1400px] flex-col px-4 pb-10 pt-4 sm:px-6 lg:px-8">
          <SiteHeader currentPath="/" />

          <main className="flex flex-1 flex-col">
            {/* Hero Section */}
            <section className="relative py-16 lg:py-24">
              <div className="mx-auto max-w-5xl text-center">
                <BlurFade delay={100}>
                  <span className="eyebrow inline-flex items-center gap-2 rounded-full border border-[var(--border-brand)] bg-[var(--brand-subtle)] px-4 py-1.5 text-xs font-semibold uppercase tracking-wider text-[var(--brand-bright)]">
                    <span className="h-1.5 w-1.5 rounded-full bg-[var(--brand)] animate-pulse" />
                    Where video becomes citable
                  </span>
                </BlurFade>

                <BlurFade delay={200}>
                  <h1 className="mt-8 text-4xl font-bold leading-[1.1] tracking-tight text-[var(--foreground)] sm:text-5xl lg:text-7xl">
                    Stop scrubbing.{" "}
                    <TextGradient animate>Start searching.</TextGradient>
                  </h1>
                </BlurFade>

                <BlurFade delay={300}>
                  <p className="mx-auto mt-6 max-w-2xl text-lg leading-relaxed text-[var(--foreground-secondary)] sm:text-xl">
                    The video search API for agents — meaning-based, grounded,
                    timestamped.
                  </p>
                </BlurFade>

                <BlurFade delay={400}>
                  <div className="mt-8 flex flex-wrap items-center justify-center gap-4">
                    <Link href="/login?mode=signup" className="button-gradient">
                      Get API key
                    </Link>
                    <a
                      href="https://discord.gg/qHDEMQB9vN"
                      target="_blank"
                      rel="noreferrer"
                      className="button-discord"
                    >
                      <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                        <path d="M20.317 4.369a19.791 19.791 0 0 0-4.885-1.515.074.074 0 0 0-.079.037c-.211.375-.444.865-.608 1.25a18.27 18.27 0 0 0-5.487 0 12.64 12.64 0 0 0-.617-1.25.077.077 0 0 0-.079-.037A19.736 19.736 0 0 0 3.677 4.369a.07.07 0 0 0-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 0 0 .031.056 19.9 19.9 0 0 0 5.993 3.03.078.078 0 0 0 .084-.028c.462-.63.874-1.295 1.226-1.994a.076.076 0 0 0-.041-.105 13.107 13.107 0 0 1-1.872-.892.077.077 0 0 1-.008-.128c.126-.094.252-.192.372-.291a.074.074 0 0 1 .077-.01c3.928 1.793 8.18 1.793 12.061 0a.074.074 0 0 1 .078.009c.12.099.246.198.373.292a.077.077 0 0 1-.006.128 12.299 12.299 0 0 1-1.873.891.077.077 0 0 0-.041.106c.36.699.772 1.364 1.225 1.994a.076.076 0 0 0 .084.028 19.84 19.84 0 0 0 6.002-3.03.077.077 0 0 0 .032-.055c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 0 0-.031-.028zM8.02 15.331c-1.182 0-2.157-1.086-2.157-2.42 0-1.332.956-2.418 2.157-2.418 1.21 0 2.176 1.096 2.157 2.418 0 1.334-.956 2.42-2.157 2.42zm7.975 0c-1.183 0-2.157-1.086-2.157-2.42 0-1.332.955-2.418 2.157-2.418 1.21 0 2.176 1.096 2.157 2.418 0 1.334-.946 2.42-2.157 2.42z" />
                      </svg>
                      Join Discord
                    </a>
                  </div>
                </BlurFade>

              </div>

              {/* API Demo — two-column layout */}
              <FadeIn delay={500} className="mt-20">
                <div className="mx-auto grid items-start gap-8 lg:grid-cols-2">
                  {/* Left: curl terminal + description */}
                  <div className="min-w-0 flex flex-col">
                    <div className="max-w-full overflow-hidden rounded-2xl border border-[var(--border)] bg-[#1a1614] shadow-xl">
                      <div className="flex items-center gap-3 border-b border-white/10 px-5 py-3.5">
                        <div className="flex gap-1.5">
                          <span className="h-3 w-3 rounded-full bg-red-400/80" />
                          <span className="h-3 w-3 rounded-full bg-amber-400/80" />
                          <span className="h-3 w-3 rounded-full bg-emerald-400/80" />
                        </div>
                        <span className="rounded-md border border-white/10 bg-white/5 px-3 py-1 text-[10px] font-semibold uppercase tracking-wider text-white/60">
                          POST /v1/search
                        </span>
                        <span className="ml-auto text-xs text-white/30">search.sh</span>
                      </div>
                      <pre className="overflow-x-auto px-6 py-8 font-mono text-sm leading-relaxed text-[#e8e4dc] lg:text-base">
                        <code>{`curl "https://api.cerul.ai/v1/search" \\
  -H "Authorization: Bearer YOUR_CERUL_API_KEY" \\
  -d '{
    "query": "Sam Altman views on AI video generation tools"
  }'`}</code>
                      </pre>
                    </div>

                    <p className="mt-5 text-sm leading-relaxed text-[var(--foreground-secondary)]">
                      One API call. Cerul searches across speech, visual scenes, and on-screen text
                      — then returns timestamped, grounded results your agent can act on.
                    </p>

                    <div className="mt-6 flex flex-wrap items-center gap-4">
                      <Link href="/login?mode=signup" className="button-gradient">
                        Get API key
                      </Link>
                      <Link href="/docs" className="button-secondary">
                        Read quickstart
                      </Link>
                    </div>
                  </div>

                  {/* Right: video result */}
                  <div className="min-w-0 flex flex-col rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-4 shadow-xl">
                    {/* Video thumbnail */}
                    <a
                      href="https://cerul.ai/v/homepage1"
                      target="_blank"
                      rel="noopener noreferrer"
                      className="group relative block aspect-[16/10] overflow-hidden rounded-xl bg-[#1a1614]"
                    >
                      <Image
                        src="https://cdn.cerul.ai/homepage/sam-altman-lex-fridman-20m23s.webp"
                        alt="Sam Altman on AI video generation — Lex Fridman Podcast"
                        fill
                        priority
                        className="object-cover transition-transform duration-500 group-hover:scale-105"
                        sizes="(max-width: 1024px) 100vw, 50vw"
                      />
                      <div className="absolute inset-0 flex items-center justify-center">
                        <div className="flex h-16 w-16 items-center justify-center rounded-full bg-red-600 shadow-lg transition-transform duration-300 group-hover:scale-110">
                          <svg width="24" height="24" viewBox="0 0 24 24" fill="white">
                            <path d="M8 5v14l11-7z" />
                          </svg>
                        </div>
                      </div>
                      <div className="absolute bottom-3 right-3 rounded bg-black/70 px-2 py-0.5 text-xs font-medium text-white">
                        20:23
                      </div>
                      <div className="absolute right-3 top-3 flex items-center gap-1.5 rounded-md bg-black/50 px-2.5 py-1 text-[10px] font-medium text-white backdrop-blur-sm">
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" className="text-red-500">
                          <path d="M23.498 6.186a3.016 3.016 0 0 0-2.122-2.136C19.505 3.545 12 3.545 12 3.545s-7.505 0-9.377.505A3.017 3.017 0 0 0 .502 6.186C0 8.07 0 12 0 12s0 3.93.502 5.814a3.016 3.016 0 0 0 2.122 2.136c1.871.505 9.376.505 9.376.505s7.505 0 9.377-.505a3.015 3.015 0 0 0 2.122-2.136C24 15.93 24 12 24 12s0-3.93-.502-5.814z" />
                          <path d="M9.545 15.568V8.432L15.818 12l-6.273 3.568z" fill="white" />
                        </svg>
                        Watch on YouTube
                      </div>
                    </a>

                    <div className="mt-4 px-1">
                      <p className="text-sm text-[var(--foreground)]">
                        <span className="font-semibold">Sam Altman on AI video generation</span>
                        <span className="mx-2 text-[var(--foreground-tertiary)]">&middot;</span>
                        <span className="text-[var(--foreground-tertiary)]">Lex Fridman Podcast</span>
                      </p>
                      <p className="mt-1 text-xs text-[var(--foreground-tertiary)]">
                        Matched at <span className="font-semibold text-[var(--foreground-secondary)]">20:23</span> for &ldquo;Sam Altman views on AI video generation tools&rdquo;
                      </p>
                    </div>
                  </div>
                </div>
              </FadeIn>
            </section>

            {/* Features Section */}
            <Features
              eyebrow="Features"
              title="Every second, searchable"
              description="The video search API for agents — meaning-based, grounded, timestamped."
              features={features}
            />

            {/* Product Split Section */}
            <section className="scroll-mt-36 py-16 lg:scroll-mt-32 lg:py-24" id="products">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  API or App
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl lg:text-5xl">
                  Two ways to use Cerul
                </h2>
                <p className="mx-auto mt-4 max-w-3xl text-lg leading-relaxed text-[var(--foreground-secondary)]">
                  Cerul API and Cerul App are two product surfaces for the same video
                  search idea. Choose the hosted API when your agents need a managed
                  endpoint. Choose the desktop app when your own media library should
                  stay local.
                </p>
              </FadeIn>

              <div className="mt-12 grid gap-6 lg:grid-cols-2">
                <FadeIn delay={100}>
                  <div className="group h-full rounded-[28px] border border-[var(--border-brand)] bg-[var(--surface-elevated)] p-7 shadow-[var(--shadow)] transition hover:-translate-y-1 hover:border-[var(--brand-bright)] sm:p-8">
                    <div className="flex flex-wrap items-start justify-between gap-4">
                      <div>
                        <span className="label label-brand">Hosted API</span>
                        <h3 className="mt-5 text-2xl font-bold tracking-tight text-[var(--foreground)]">
                          Cerul API on cerul.ai
                        </h3>
                      </div>
                      <span className="rounded-full border border-[var(--border)] bg-[var(--background-sunken)] px-3 py-1 text-xs font-semibold uppercase tracking-wider text-[var(--foreground-secondary)]">
                        Cloud
                      </span>
                    </div>
                    <p className="mt-4 text-[15px] leading-relaxed text-[var(--foreground-secondary)]">
                      This is the developer platform: API keys, docs, usage, billing,
                      and a hosted video-search endpoint at{" "}
                      <code className="rounded-md bg-[var(--background-sunken)] px-1.5 py-0.5 text-[var(--foreground)]">
                        api.cerul.ai
                      </code>
                      . It is best for agents, SaaS products, research workflows, and
                      teams that want a managed service.
                    </p>
                    <dl className="mt-6 grid gap-x-6 gap-y-4 border-y border-[var(--border)] py-5 text-sm sm:grid-cols-2">
                      <div>
                        <dt className="font-semibold text-[var(--foreground)]">Runs in</dt>
                        <dd className="mt-1 text-[var(--foreground-secondary)]">Cerul Cloud</dd>
                      </div>
                      <div>
                        <dt className="font-semibold text-[var(--foreground)]">Use it for</dt>
                        <dd className="mt-1 text-[var(--foreground-secondary)]">Agent video search API</dd>
                      </div>
                    </dl>
                    <div className="mt-7 flex flex-wrap gap-3">
                      <Link href="/docs" className="button-gradient">
                        Read API docs
                      </Link>
                      <Link href="/login?mode=signup" className="button-secondary">
                        Get API key
                      </Link>
                    </div>
                  </div>
                </FadeIn>

                <FadeIn delay={200}>
                  <div className="group h-full rounded-[28px] border border-[var(--border)] bg-[var(--surface)] p-7 shadow-[var(--shadow)] transition hover:-translate-y-1 hover:border-[var(--border-brand)] sm:p-8">
                    <div className="flex flex-wrap items-start justify-between gap-4">
                      <div>
                        <span className="label label-accent">Desktop App</span>
                        <h3 className="mt-5 text-2xl font-bold tracking-tight text-[var(--foreground)]">
                          Cerul App on app.cerul.ai
                        </h3>
                      </div>
                      <span className="rounded-full border border-[var(--border)] bg-[var(--background-sunken)] px-3 py-1 text-xs font-semibold uppercase tracking-wider text-[var(--foreground-secondary)]">
                        Local-first
                      </span>
                    </div>
                    <p className="mt-4 text-[15px] leading-relaxed text-[var(--foreground-secondary)]">
                      This is the desktop version: point it at folders, YouTube channels,
                      and podcasts, then search your own video and audio library locally.
                      It includes a global overlay and local REST API for agents running
                      on your machine.
                    </p>
                    <dl className="mt-6 grid gap-x-6 gap-y-4 border-y border-[var(--border)] py-5 text-sm sm:grid-cols-2">
                      <div>
                        <dt className="font-semibold text-[var(--foreground)]">Runs on</dt>
                        <dd className="mt-1 text-[var(--foreground-secondary)]">Your Mac</dd>
                      </div>
                      <div>
                        <dt className="font-semibold text-[var(--foreground)]">Use it for</dt>
                        <dd className="mt-1 text-[var(--foreground-secondary)]">Private media memory</dd>
                      </div>
                    </dl>
                    <div className="mt-7 flex flex-wrap gap-3">
                      <a
                        href="https://app.cerul.ai"
                        target="_blank"
                        rel="noreferrer"
                        className="button-gradient"
                      >
                        Open Cerul App site
                      </a>
                      <a
                        href="https://app.cerul.ai/download"
                        target="_blank"
                        rel="noreferrer"
                        className="button-secondary"
                      >
                        Download app
                      </a>
                    </div>
                  </div>
                </FadeIn>
              </div>
            </section>

            {/* Stats Section */}
            <section className="py-16 lg:py-24">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  Metrics
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                  Built for scale
                </h2>
              </FadeIn>

              <div className="mt-12 grid gap-6 sm:grid-cols-2 lg:grid-cols-4">
                {stats.map((stat, index) => (
                  <FadeIn key={stat.label} delay={index * 100}>
                    <div className="stat-card text-center">
                      <p className="text-4xl font-bold tracking-tight text-[var(--foreground)] lg:text-5xl">
                        <AnimatedCounter
                          value={stat.value}
                          suffix={stat.suffix}
                        />
                      </p>
                      <p className="mt-2 text-sm text-[var(--foreground-secondary)]">
                        {stat.label}
                      </p>
                    </div>
                  </FadeIn>
                ))}
              </div>
            </section>

            {/* Privacy / Trust Section */}
            <section className="py-12 lg:py-16">
              <FadeIn>
                <Link
                  href="/security"
                  className="group relative mx-auto block max-w-3xl overflow-hidden rounded-2xl border border-[var(--border)] bg-[var(--surface)] px-6 py-5 transition hover:border-[var(--border-brand)]"
                >
                  <div className="flex items-center gap-4">
                    <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-xl border border-[var(--border-brand)] bg-[var(--brand-subtle)] text-[var(--brand-bright)]">
                      <svg width="18" height="18" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                        <path strokeLinecap="round" strokeLinejoin="round" d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                      </svg>
                    </div>
                    <div className="min-w-0 flex-1">
                      <p className="text-sm font-semibold text-[var(--foreground)]">
                        Your data never trains any model.
                      </p>
                      <p className="mt-1 text-xs text-[var(--foreground-secondary)]">
                        Encryption, no-training commitment, full data control. Read our security posture →
                      </p>
                    </div>
                    <svg
                      className="h-5 w-5 flex-shrink-0 text-[var(--foreground-tertiary)] transition group-hover:translate-x-0.5 group-hover:text-[var(--brand-bright)]"
                      fill="none"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                      strokeWidth={2}
                    >
                      <path strokeLinecap="round" strokeLinejoin="round" d="M9 5l7 7-7 7" />
                    </svg>
                  </div>
                </Link>
              </FadeIn>
            </section>

            {/* CTA Section */}
            <section className="py-16 lg:py-24">
              <FadeIn>
                <div className="relative overflow-hidden rounded-[32px] border border-[var(--border)] bg-gradient-to-br from-[var(--surface)] via-[var(--background)] to-[var(--surface)] p-12 text-center lg:p-16">
                  <div className="absolute inset-0 bg-[radial-gradient(circle_at_center,rgba(111, 151, 201,0.08),transparent_50%)]" />
                  <div className="relative">
                    <h2 className="text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl lg:text-5xl">
                      Give your agents eyes on{" "}
                      <span className="text-[var(--brand-bright)]">
                        every video
                      </span>
                    </h2>
                    <p className="mx-auto mt-4 max-w-xl text-lg text-[var(--foreground-secondary)]">
                      Start building today with our free tier. No credit card required.
                    </p>
                    <div className="mt-8 flex flex-wrap items-center justify-center gap-4">
                      <Link href="/login?mode=signup" className="button-gradient">
                        Get started
                      </Link>
                      <Link href="/docs" className="button-secondary">
                        Read the docs
                      </Link>
                      <a
                        href="https://discord.gg/qHDEMQB9vN"
                        target="_blank"
                        rel="noreferrer"
                        className="button-secondary"
                      >
                        Join Discord
                      </a>
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
