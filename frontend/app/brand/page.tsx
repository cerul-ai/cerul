import type { Metadata } from "next";
import Image from "next/image";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import { BlurFade } from "@/components/animations";

export const metadata: Metadata = {
  title: "Brand & Press Kit",
  description:
    "Cerul logos, icons, social cards, brand guidelines, and a downloadable press kit for journalists, newsletters, and product directories.",
  alternates: {
    canonical: "/brand",
  },
};

const logoAssets = [
  {
    label: "Primary mark",
    description: "Flat mark on light backgrounds",
    svg: "/press-kit/01-logos/vector-svg/cerul-primary-black.svg",
    bg: "#eef1f4",
  },
  {
    label: "Inverted mark",
    description: "Flat white mark on dark backgrounds",
    svg: "/press-kit/01-logos/vector-svg/cerul-primary-white.svg",
    bg: "#15171c",
  },
  {
    label: "App icon (default)",
    description: "Graphite container — the default Cerul icon",
    svg: "/press-kit/01-logos/vector-svg/cerul-primary-color.svg",
    bg: "#ffffff",
  },
  {
    label: "Wordmark",
    description: "Cerul in Inter",
    svg: "/press-kit/01-logos/vector-svg/cerul-wordmark.svg",
    bg: "#eef1f4",
  },
];

const screenshots = [
  {
    label: "Homepage",
    description: "Marketing hero — best for cover images",
    src: "/press-kit/05-product-screenshots/screenshot-home.png",
    width: 2400,
    height: 1500,
  },
  {
    label: "Agent skill — search",
    description: "Claude Code calling the Cerul skill",
    src: "/press-kit/05-product-screenshots/agent-skill-search.png",
    width: 1600,
    height: 1000,
  },
  {
    label: "Agent skill — result",
    description: "Agent synthesizing video evidence into an answer",
    src: "/press-kit/05-product-screenshots/agent-skill-result.png",
    width: 1600,
    height: 1000,
  },
  {
    label: "CLI",
    description: "cerul search output with inline frames",
    src: "/press-kit/05-product-screenshots/cli-search.png",
    width: 1600,
    height: 1000,
  },
];

const usageShots = [
  {
    label: "CLI — Sam Altman search",
    description:
      "cerul search finds the exact moment Altman said \"compute is the new oil\" with an inline frame preview",
    src: "/press-kit/05-product-screenshots/usage/cli-sam-altman-search.png",
  },
  {
    label: "CLI — Dario Amodei result",
    description:
      "Inline video frame rendered in-terminal (iTerm2 / WezTerm / Kitty)",
    src: "/press-kit/05-product-screenshots/usage/cli-dario-amodei-result.png",
  },
  {
    label: "Claude Code — research workflow",
    description:
      "Claude Code (Opus 4.6) running the Cerul skill across multiple sub-topic searches to prep for a Demis Hassabis interview",
    src: "/press-kit/05-product-screenshots/usage/claude-code-demis-research.png",
  },
  {
    label: "Claude Code — synthesized notes",
    description:
      "Agent-generated research brief with bullet points, timestamps, and cerul.ai/v/ citations",
    src: "/press-kit/05-product-screenshots/usage/claude-code-research-notes.png",
  },
  {
    label: "Telegram bot — Chinese query",
    description:
      "A Telegram bot powered by Cerul answering \"How does Dario Amodei talk about responsible scaling?\" with sourced video links",
    src: "/press-kit/05-product-screenshots/usage/telegram-bot-dario-query.jpg",
  },
];

const colorSwatches = [
  { name: "Graphite", hex: "#15171c" },
  { name: "Copper (accent)", hex: "#a85a28" },
  { name: "Copper · on dark", hex: "#d99a62" },
  { name: "Paper", hex: "#eef0f4" },
];

export default function BrandPage() {
  return (
    <div className="soft-theme">
      <div className="mx-auto flex min-h-screen max-w-[1400px] flex-col px-4 pb-8 pt-4 sm:px-6 lg:px-8">
        <SiteHeader currentPath="/brand" />
        <main className="flex-1">
          <section className="py-16 lg:py-24">
            <BlurFade>
              <span className="eyebrow inline-flex items-center gap-2">
                <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                Brand & Press
              </span>
            </BlurFade>
            <BlurFade delay={100}>
              <h1 className="mt-6 text-4xl font-bold tracking-tight text-[var(--foreground)] sm:text-5xl">
                Press Kit
              </h1>
            </BlurFade>
            <BlurFade delay={200}>
              <p className="mt-6 max-w-2xl text-[17px] leading-relaxed text-[var(--foreground-secondary)]">
                Writing about Cerul? Everything you need — logos, icons, social
                cards, and brand guidelines — is bundled below. Pick individual
                assets, or download the whole kit in one zip. Need something
                custom? Email{" "}
                <a
                  href="mailto:support@cerul.ai"
                  className="text-[var(--brand-bright)] hover:underline"
                >
                  support@cerul.ai
                </a>
                .
              </p>
            </BlurFade>

            <BlurFade delay={300}>
              <div className="mt-10 flex flex-wrap gap-3">
                <a
                  href="/cerul-press-kit.zip"
                  download
                  style={{ color: "#ffffff" }}
                  className="inline-flex items-center gap-2 rounded-full bg-[var(--foreground)] px-6 py-3 text-sm font-semibold transition hover:opacity-90"
                >
                  <svg
                    width="16"
                    height="16"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  >
                    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
                    <polyline points="7 10 12 15 17 10" />
                    <line x1="12" y1="15" x2="12" y2="3" />
                  </svg>
                  Download full press kit (.zip)
                </a>
                <a
                  href="/press-kit/BRAND.md"
                  className="inline-flex items-center gap-2 rounded-full border border-[var(--border)] bg-[var(--surface)] px-6 py-3 text-sm font-semibold text-[var(--foreground)] transition hover:border-[var(--border-brand)]"
                >
                  Brand guidelines
                </a>
              </div>
            </BlurFade>

            {/* One-liner */}
            <BlurFade delay={400}>
              <div className="mt-16 max-w-3xl rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-8">
                <h2 className="text-sm font-semibold uppercase tracking-wide text-[var(--foreground-tertiary)]">
                  One-liner
                </h2>
                <p className="mt-4 text-lg leading-relaxed text-[var(--foreground)]">
                  Cerul is where video becomes citable — search video
                  by meaning across speech, visuals, and on-screen text.
                </p>
              </div>
            </BlurFade>

            {/* Logos */}
            <BlurFade delay={500}>
              <h2 className="mt-20 text-2xl font-bold tracking-tight text-[var(--foreground)]">
                Logos
              </h2>
              <p className="mt-3 max-w-2xl text-[15px] text-[var(--foreground-secondary)]">
                Right-click any logo to save it, or use the download link
                underneath. Please don&rsquo;t recolor, stretch, or distort.
              </p>
              <div className="mt-8 grid gap-6 sm:grid-cols-2">
                {logoAssets.map((asset) => (
                  <div
                    key={asset.label}
                    className="flex flex-col overflow-hidden rounded-2xl border border-[var(--border)] bg-[var(--surface)]"
                  >
                    <div
                      className="flex h-48 items-center justify-center"
                      style={{ backgroundColor: asset.bg }}
                    >
                      <Image
                        src={asset.svg}
                        alt={asset.label}
                        width={160}
                        height={160}
                        unoptimized
                        className="max-h-24 w-auto"
                      />
                    </div>
                    <div className="flex items-center justify-between border-t border-[var(--border)] px-5 py-4">
                      <div>
                        <p className="text-sm font-semibold text-[var(--foreground)]">
                          {asset.label}
                        </p>
                        <p className="text-xs text-[var(--foreground-tertiary)]">
                          {asset.description}
                        </p>
                      </div>
                      <a
                        href={asset.svg}
                        download
                        className="text-xs font-semibold text-[var(--brand-bright)] hover:underline"
                      >
                        Download SVG
                      </a>
                    </div>
                  </div>
                ))}
              </div>
            </BlurFade>

            {/* Screenshots */}
            <BlurFade delay={550}>
              <h2 className="mt-20 text-2xl font-bold tracking-tight text-[var(--foreground)]">
                Product screenshots
              </h2>
              <p className="mt-3 max-w-2xl text-[15px] text-[var(--foreground-secondary)]">
                Publish-ready PNGs of the homepage, agent skill, and CLI. Use
                these as cover images or inline figures in articles.
              </p>
              <div className="mt-8 grid gap-6 sm:grid-cols-2">
                {screenshots.map((shot) => (
                  <div
                    key={shot.src}
                    className="flex flex-col overflow-hidden rounded-2xl border border-[var(--border)] bg-[var(--surface)]"
                  >
                    <div className="relative aspect-[16/10] w-full overflow-hidden bg-[var(--background-sunken)]">
                      <Image
                        src={shot.src}
                        alt={shot.label}
                        fill
                        sizes="(min-width: 640px) 50vw, 100vw"
                        className="object-cover"
                      />
                    </div>
                    <div className="flex items-center justify-between border-t border-[var(--border)] px-5 py-4">
                      <div>
                        <p className="text-sm font-semibold text-[var(--foreground)]">
                          {shot.label}
                        </p>
                        <p className="text-xs text-[var(--foreground-tertiary)]">
                          {shot.description}
                        </p>
                      </div>
                      <a
                        href={shot.src}
                        download
                        className="text-xs font-semibold text-[var(--brand-bright)] hover:underline"
                      >
                        Download PNG
                      </a>
                    </div>
                  </div>
                ))}
              </div>
            </BlurFade>

            {/* Real-world usage */}
            <BlurFade delay={560}>
              <h2 className="mt-20 text-2xl font-bold tracking-tight text-[var(--foreground)]">
                Real-world usage
              </h2>
              <p className="mt-3 max-w-2xl text-[15px] text-[var(--foreground-secondary)]">
                Shots of Cerul being used in the wild — inside the CLI, inside
                Claude Code as a skill, and inside a Telegram bot. Use these
                instead of marketing screenshots when you want to show what the
                developer experience actually looks like.
              </p>
              <div className="mt-8 grid gap-6 sm:grid-cols-2">
                {usageShots.map((shot) => (
                  <div
                    key={shot.src}
                    className="flex flex-col overflow-hidden rounded-2xl border border-[var(--border)] bg-[var(--surface)]"
                  >
                    <div className="relative aspect-[16/10] w-full overflow-hidden bg-[var(--background-sunken)]">
                      <Image
                        src={shot.src}
                        alt={shot.label}
                        fill
                        sizes="(min-width: 640px) 50vw, 100vw"
                        className="object-contain"
                      />
                    </div>
                    <div className="flex items-center justify-between gap-4 border-t border-[var(--border)] px-5 py-4">
                      <div className="min-w-0">
                        <p className="text-sm font-semibold text-[var(--foreground)]">
                          {shot.label}
                        </p>
                        <p className="text-xs leading-relaxed text-[var(--foreground-tertiary)]">
                          {shot.description}
                        </p>
                      </div>
                      <a
                        href={shot.src}
                        download
                        className="shrink-0 text-xs font-semibold text-[var(--brand-bright)] hover:underline"
                      >
                        Download
                      </a>
                    </div>
                  </div>
                ))}
              </div>
            </BlurFade>

            {/* Demo video */}
            <BlurFade delay={575}>
              <h2 className="mt-20 text-2xl font-bold tracking-tight text-[var(--foreground)]">
                Demo video
              </h2>
              <p className="mt-3 max-w-2xl text-[15px] text-[var(--foreground-secondary)]">
                A short, loopable product demo. Embed in posts, tweets, or
                newsletters.
              </p>
              <div className="mt-8 overflow-hidden rounded-2xl border border-[var(--border)] bg-[var(--surface)]">
                <video
                  src="/press-kit/video/demo.mp4"
                  controls
                  loop
                  muted
                  playsInline
                  preload="metadata"
                  className="aspect-video w-full bg-black"
                />
                <div className="flex items-center justify-between border-t border-[var(--border)] px-5 py-4">
                  <div>
                    <p className="text-sm font-semibold text-[var(--foreground)]">
                      Product demo (MP4)
                    </p>
                    <p className="text-xs text-[var(--foreground-tertiary)]">
                      ≈7 MB · loopable · no audio required
                    </p>
                  </div>
                  <a
                    href="/press-kit/video/demo.mp4"
                    download
                    className="text-xs font-semibold text-[var(--brand-bright)] hover:underline"
                  >
                    Download MP4
                  </a>
                </div>
              </div>
            </BlurFade>

            {/* Colors */}
            <BlurFade delay={600}>
              <h2 className="mt-20 text-2xl font-bold tracking-tight text-[var(--foreground)]">
                Colors
              </h2>
              <div className="mt-8 grid gap-4 sm:grid-cols-2 lg:grid-cols-5">
                {colorSwatches.map((swatch) => (
                  <div
                    key={swatch.hex}
                    className="overflow-hidden rounded-2xl border border-[var(--border)]"
                  >
                    <div
                      className="h-24"
                      style={{ backgroundColor: swatch.hex }}
                    />
                    <div className="bg-[var(--surface)] px-4 py-3">
                      <p className="text-sm font-semibold text-[var(--foreground)]">
                        {swatch.name}
                      </p>
                      <p className="font-mono text-xs text-[var(--foreground-tertiary)]">
                        {swatch.hex.toUpperCase()}
                      </p>
                    </div>
                  </div>
                ))}
              </div>
            </BlurFade>

            {/* Quick facts */}
            <BlurFade delay={700}>
              <h2 className="mt-20 text-2xl font-bold tracking-tight text-[var(--foreground)]">
                Quick facts
              </h2>
              <dl className="mt-8 grid max-w-3xl gap-6 sm:grid-cols-2">
                <div>
                  <dt className="text-xs font-semibold uppercase tracking-wide text-[var(--foreground-tertiary)]">
                    Product
                  </dt>
                  <dd className="mt-2 text-[15px] text-[var(--foreground)]">
                    Video understanding search API for AI agents.
                  </dd>
                </div>
                <div>
                  <dt className="text-xs font-semibold uppercase tracking-wide text-[var(--foreground-tertiary)]">
                    Pronunciation
                  </dt>
                  <dd className="mt-2 text-[15px] text-[var(--foreground)]">
                    <em>SER-uhl</em>
                  </dd>
                </div>
                <div>
                  <dt className="text-xs font-semibold uppercase tracking-wide text-[var(--foreground-tertiary)]">
                    Website
                  </dt>
                  <dd className="mt-2 text-[15px] text-[var(--foreground)]">
                    cerul.ai
                  </dd>
                </div>
                <div>
                  <dt className="text-xs font-semibold uppercase tracking-wide text-[var(--foreground-tertiary)]">
                    Contact
                  </dt>
                  <dd className="mt-2 text-[15px] text-[var(--foreground)]">
                    <a
                      href="mailto:support@cerul.ai"
                      className="text-[var(--brand-bright)] hover:underline"
                    >
                      support@cerul.ai
                    </a>
                  </dd>
                </div>
              </dl>
            </BlurFade>
          </section>
        </main>
        <SiteFooter />
      </div>
    </div>
  );
}
