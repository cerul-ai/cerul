import type { Metadata } from "next";
import Link from "next/link";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import {
  FadeIn,
  BlurFade,
  TextGradient,
  AnimatedCounter,
} from "@/components/animations";
import { getSiteOrigin } from "@/lib/site-url";

const enterpriseDescription =
  "Cerul Enterprise — production-grade video search for companies with private archives. Deploy in your cloud or ours. SSO, SLA, custom indexing, dedicated support.";

const siteOrigin = getSiteOrigin();

export const metadata: Metadata = {
  title: "Enterprise",
  description: enterpriseDescription,
  alternates: {
    canonical: "/enterprise",
  },
  openGraph: {
    title: "Cerul Enterprise — Production video search for your archive",
    description: enterpriseDescription,
    url: `${siteOrigin}/enterprise`,
    siteName: "Cerul",
    type: "website",
  },
  twitter: {
    card: "summary_large_image",
    title: "Cerul Enterprise",
    description: enterpriseDescription,
  },
};

const stats = [
  { value: 99.9, suffix: "%", label: "Uptime SLA" },
  { value: 150, suffix: "ms", label: "Avg. search latency" },
  { value: 10000, suffix: "+", label: "Hours indexed daily" },
  { value: 24, suffix: "/7", label: "Dedicated support" },
];

const useCases = [
  {
    industry: "Education & Training",
    audience: "Online learning platforms, corporate training, certification",
    examples: [
      "Find every moment a lecturer explains backpropagation",
      "Surface the exact second a compliance topic is covered",
      "Answer learner questions with video citations",
    ],
  },
  {
    industry: "Media & Publishing",
    audience: "Broadcasters, news organizations, video archives",
    examples: [
      "Locate B-roll matching a news story in seconds",
      "Search decades of footage by spoken topic or on-screen visual",
      "Surface clips for content licensing requests",
    ],
  },
  {
    industry: "Video SaaS",
    audience: "Meeting recorders, sales call platforms, video CMS",
    examples: [
      "Add semantic search across all your customers' recordings",
      "Power AI summaries with grounded video citations",
      "Make meeting libraries instantly queryable",
    ],
  },
  {
    industry: "Stock & Footage",
    audience: "Stock libraries, archive houses, production companies",
    examples: [
      "Index millions of clips by visual + spoken content",
      "Replace tag-based search with semantic retrieval",
      "Enable natural-language clip discovery for creators",
    ],
  },
  {
    industry: "Internal Knowledge",
    audience: "All-hands archives, Zoom recordings, training videos",
    examples: [
      "Search every all-hands meeting your company has ever had",
      "Surface decisions made in past planning sessions",
      "Onboard new hires with searchable historical context",
    ],
  },
  {
    industry: "Legal & Compliance",
    audience: "Depositions, recorded calls, regulatory archives",
    examples: [
      "Search depositions for testimony on a specific topic",
      "Find compliance discussions across recorded calls",
      "Build defensible records of what was said and when",
    ],
  },
];

const capabilities = [
  {
    title: "Visual + speech + OCR fusion",
    description:
      "Index what is said, what is shown on screen, and what is written in slides or code — all aligned to a single timeline.",
  },
  {
    title: "Multimodal embeddings",
    description:
      "State-of-the-art encoders (SigLIP, Voyage, or your choice) power retrieval that understands meaning, not just keywords.",
  },
  {
    title: "Tuned reranker",
    description:
      "Our domain-aware reranking model boosts relevance by 30-50% over standard hybrid retrieval — only available in Enterprise and Cloud.",
  },
  {
    title: "Agent-ready API",
    description:
      "JSON-native REST endpoints with stable contracts. Designed for Claude Code, Cursor, autonomous agents, and your internal tools.",
  },
  {
    title: "Private indexing",
    description:
      "Your videos and embeddings never leave your environment. Index runs in your VPC, on-prem, or in our dedicated tenant.",
  },
  {
    title: "Custom embedding models",
    description:
      "Bring your own fine-tuned encoder for domain-specific accuracy on medical, legal, scientific, or technical content.",
  },
  {
    title: "Bulk ingestion",
    description:
      "Process millions of hours of historical video with parallel pipelines. We've benchmarked at 10,000+ hours indexed per day per cluster.",
  },
  {
    title: "Incremental sync",
    description:
      "New content gets indexed within minutes of upload. Sources include S3, Azure Blob, GCS, Zoom Cloud, YouTube channels, RSS, and custom integrations.",
  },
];

const deploymentOptions = [
  {
    name: "Dedicated SaaS",
    tagline: "Fastest to deploy",
    description:
      "We run everything in a dedicated tenant on our infrastructure. Single-tenant isolation, dedicated resources, your data stays in your region.",
    bestFor: "Teams that want managed infrastructure with isolation guarantees.",
    features: [
      "Single-tenant deployment",
      "Region selection (US, EU, APAC)",
      "Managed updates and scaling",
      "99.9% uptime SLA",
      "Daily-to-go-live in 2 weeks",
    ],
    accent: "brand",
  },
  {
    name: "Private Cloud",
    tagline: "Your AWS / GCP / Azure",
    description:
      "We deploy Cerul inside your cloud account. Your network, your IAM, your audit logs. We provide updates, support, and tuning.",
    bestFor: "Regulated industries, sensitive workloads, existing cloud footprint.",
    features: [
      "Runs in your cloud account",
      "Your VPC, IAM, and KMS",
      "Terraform / Pulumi modules included",
      "We handle updates remotely",
      "Daily-to-go-live in 4–6 weeks",
    ],
    accent: "accent",
    popular: true,
  },
  {
    name: "On-Premise",
    tagline: "Air-gapped capable",
    description:
      "Full on-prem deployment for environments without cloud access. Includes Helm charts, offline model bundles, and white-glove install.",
    bestFor: "Government, defense, sovereign data, fully air-gapped networks.",
    features: [
      "Kubernetes / bare metal",
      "Offline model bundles",
      "Air-gapped install path",
      "On-site or remote install support",
      "Daily-to-go-live in 8–12 weeks",
    ],
    accent: "ink",
  },
];

const securityFeatures = [
  {
    title: "SOC 2 Type II",
    description: "Audit in progress; controls inherited from our cloud infrastructure today.",
  },
  {
    title: "SSO / SAML / SCIM",
    description: "Integrate with Okta, Google Workspace, Azure AD, JumpCloud, OneLogin.",
  },
  {
    title: "Audit logs",
    description: "Every query, ingestion event, and admin action logged and exportable.",
  },
  {
    title: "Encryption",
    description: "TLS 1.3 in transit, AES-256 at rest. BYOK with AWS KMS / Azure Key Vault.",
  },
  {
    title: "Role-based access",
    description: "Fine-grained RBAC for teams, sources, and indexes.",
  },
  {
    title: "Data residency",
    description: "Choose region; full data isolation between tenants in Dedicated SaaS.",
  },
];

const buildVsBuy = [
  {
    category: "Engineering time",
    build: "6+ months, 2 senior engineers",
    buy: "2–8 weeks (depending on deployment)",
  },
  {
    category: "Upfront cost",
    build: "$300k – $600k (loaded)",
    buy: "Setup fee + annual contract",
  },
  {
    category: "Ongoing maintenance",
    build: "0.5–1 FTE forever",
    buy: "Included in support contract",
  },
  {
    category: "Model upgrades",
    build: "Each upgrade = re-architecture project",
    buy: "Handled by us; you stay current",
  },
  {
    category: "Quality (retrieval relevance)",
    build: "Baseline open-source models, no tuning",
    buy: "Tuned reranker + multimodal fusion",
  },
  {
    category: "Time to first result",
    build: "9–12 months",
    buy: "Days to weeks",
  },
  {
    category: "Risk",
    build: "Model selection, scale, ops all on you",
    buy: "Production-grade, with SLA",
  },
];

const process = [
  {
    step: "01",
    title: "Discovery call",
    description:
      "30 minutes. We learn your video corpus, search use case, security requirements, and deployment constraints.",
  },
  {
    step: "02",
    title: "Scoped pilot",
    description:
      "We index a representative sample (typically 100–1,000 hours) and you evaluate retrieval quality against your real queries.",
  },
  {
    step: "03",
    title: "Production deployment",
    description:
      "Choose deployment model (SaaS, Private Cloud, or On-Prem). We deploy, integrate auth and storage, and run your full corpus through the pipeline.",
  },
  {
    step: "04",
    title: "Ongoing partnership",
    description:
      "Dedicated support, regular model upgrades, custom feature work, and quarterly business reviews. You scale, we maintain.",
  },
];

const enterpriseFaqs = [
  {
    question: "How is Enterprise different from Pro?",
    answer:
      "Pro is a self-serve subscription against our public hosted API and corpus. Enterprise is a private deployment of the full Cerul stack — your own indexes, your own data, your own infrastructure (in your cloud, ours, or on-prem). Enterprise also unlocks the tuned reranker, custom embedding models, SSO/SAML, audit logs, and a dedicated support channel.",
  },
  {
    question: "Can we deploy Cerul in our own AWS / GCP / Azure account?",
    answer:
      "Yes. Private Cloud deployment runs Cerul entirely inside your cloud account using Terraform / Pulumi modules we provide. Your VPC, your IAM, your KMS. We handle updates remotely via your standard CI/CD pipelines.",
  },
  {
    question: "Do you support air-gapped or on-premise deployments?",
    answer:
      "Yes. We provide Helm charts, offline model bundles, and white-glove install support. Common for government, defense, regulated banking, and sovereign data environments.",
  },
  {
    question: "What kind of video sources can you ingest?",
    answer:
      "Out of the box: S3, Azure Blob, GCS, local filesystem, YouTube channels, RSS feeds, Zoom Cloud Recordings, Loom, Vimeo. Custom integrations available — we've built ingestion for sales call platforms (Gong, Chorus), meeting recorders (Otter, Fathom), and proprietary DAMs.",
  },
  {
    question: "What about ongoing model upgrades?",
    answer:
      "Embedding and reranking models improve every 6–12 months. We re-embed your corpus in the background as part of your support contract — you get the quality upgrade without any work on your side. We never break query compatibility.",
  },
  {
    question: "Can we fine-tune the embedding model on our data?",
    answer:
      "Yes. For domain-specific corpora (medical, legal, scientific, technical), we offer custom embedding model training as an add-on. Typical improvement is 20–40% retrieval accuracy on domain queries.",
  },
  {
    question: "How does pricing work for Enterprise?",
    answer:
      "Pricing is based on three factors: total hours indexed, query volume, and deployment model. Most customers land between $30k–$300k/year. We start with a scoped pilot to validate fit before any long-term commitment.",
  },
  {
    question: "What's the time to first production result?",
    answer:
      "Dedicated SaaS: 2 weeks. Private Cloud: 4–6 weeks. On-Premise: 8–12 weeks. Pilots typically start within a week of a signed mutual NDA.",
  },
];

const webPageJsonLd = {
  "@context": "https://schema.org",
  "@type": "WebPage",
  name: "Cerul Enterprise",
  description: enterpriseDescription,
  url: `${siteOrigin}/enterprise`,
  isPartOf: {
    "@type": "WebSite",
    name: "Cerul",
    url: siteOrigin,
  },
};

const faqJsonLd = {
  "@context": "https://schema.org",
  "@type": "FAQPage",
  mainEntity: enterpriseFaqs.map((faq) => ({
    "@type": "Question",
    name: faq.question,
    acceptedAnswer: {
      "@type": "Answer",
      text: faq.answer,
    },
  })),
};

export default function EnterprisePage() {
  return (
    <>
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(webPageJsonLd) }}
      />
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(faqJsonLd) }}
      />
      <div className="soft-theme overflow-x-clip">
        <div className="mx-auto flex min-h-screen max-w-[1400px] flex-col px-4 pb-10 pt-4 sm:px-6 lg:px-8">
          <SiteHeader currentPath="/enterprise" />

          <main className="flex flex-1 flex-col">
            {/* Hero */}
            <section className="relative py-16 lg:py-24">
              <div className="mx-auto max-w-5xl text-center">
                <BlurFade delay={100}>
                  <span className="eyebrow inline-flex items-center gap-2 rounded-full border border-[var(--border-brand)] bg-[var(--brand-subtle)] px-4 py-1.5 text-xs font-semibold uppercase tracking-wider text-[var(--brand-bright)]">
                    <span className="h-1.5 w-1.5 rounded-full bg-[var(--brand)] animate-pulse" />
                    Cerul Enterprise
                  </span>
                </BlurFade>

                <BlurFade delay={200}>
                  <h1 className="mt-8 text-4xl font-bold leading-[1.1] tracking-tight text-[var(--foreground)] sm:text-5xl lg:text-7xl">
                    Search every second of{" "}
                    <TextGradient animate>your video archive</TextGradient>
                  </h1>
                </BlurFade>

                <BlurFade delay={300}>
                  <p className="mx-auto mt-6 max-w-3xl text-lg leading-relaxed text-[var(--foreground-secondary)] sm:text-xl">
                    Production-grade video search infrastructure for companies with private archives.
                    Deploy in your cloud or ours. SSO, SLA, custom indexing, dedicated support.
                  </p>
                </BlurFade>

                <BlurFade delay={400}>
                  <div className="mt-10 flex flex-wrap items-center justify-center gap-4">
                    <a
                      href="mailto:panda@cerul.ai?subject=Cerul%20Enterprise%20Demo%20Request"
                      className="button-gradient"
                    >
                      Book a demo
                    </a>
                    <a href="#deployment" className="button-secondary">
                      See deployment options
                    </a>
                  </div>
                </BlurFade>
              </div>

              {/* Stats */}
              <FadeIn delay={500}>
                <div className="mt-20 grid gap-6 sm:grid-cols-2 lg:grid-cols-4">
                  {stats.map((stat, index) => (
                    <FadeIn key={stat.label} delay={500 + index * 80}>
                      <div className="stat-card text-center">
                        <p className="text-4xl font-bold tracking-tight text-[var(--foreground)] lg:text-5xl">
                          <AnimatedCounter value={stat.value} suffix={stat.suffix} />
                        </p>
                        <p className="mt-2 text-sm text-[var(--foreground-secondary)]">
                          {stat.label}
                        </p>
                      </div>
                    </FadeIn>
                  ))}
                </div>
              </FadeIn>
            </section>

            {/* The problem */}
            <section className="py-16 lg:py-24">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  The problem
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                  Your video archive is dark data
                </h2>
                <p className="mx-auto mt-4 max-w-2xl text-[var(--foreground-secondary)]">
                  Companies are accumulating thousands of hours of video — training, all-hands,
                  sales calls, customer recordings, lectures, productions — and almost none of it
                  is actually queryable.
                </p>
              </FadeIn>

              <div className="mt-12 grid gap-6 lg:grid-cols-3">
                {[
                  {
                    icon: (
                      <svg width="24" height="24" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                        <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z" />
                      </svg>
                    ),
                    title: "Knowledge is locked in pixels",
                    description:
                      "Every meeting decision, training explanation, and customer insight is trapped behind a 60-minute scrub bar. Your team's institutional memory is technically present but practically unreachable.",
                  },
                  {
                    icon: (
                      <svg width="24" height="24" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                        <path strokeLinecap="round" strokeLinejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z" />
                      </svg>
                    ),
                    title: "AI agents can't see video",
                    description:
                      "Your Claude Code, Cursor, and internal agents have web search, code search, and document search — but no way to ground answers in video. Your video archive is invisible to your AI stack.",
                  },
                  {
                    icon: (
                      <svg width="24" height="24" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                        <path strokeLinecap="round" strokeLinejoin="round" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0zM9.75 9.75l4.5 4.5m0-4.5l-4.5 4.5" />
                      </svg>
                    ),
                    title: "Building it yourself is a year",
                    description:
                      "Pipelines for transcription, frame extraction, multimodal embedding, hybrid retrieval, reranking, indexing, and a maintainable API surface — typical buildout is 6–12 months with two senior engineers, plus ongoing maintenance.",
                  },
                ].map((problem, index) => (
                  <FadeIn key={problem.title} delay={index * 100}>
                    <div className="h-full rounded-3xl border border-[var(--border)] bg-[var(--surface)] p-8 transition hover:border-[var(--border-strong)]">
                      <div className="flex h-12 w-12 items-center justify-center rounded-2xl border border-[var(--border-brand)] bg-[var(--brand-subtle)] text-[var(--brand-bright)]">
                        {problem.icon}
                      </div>
                      <h3 className="mt-6 text-lg font-semibold text-[var(--foreground)]">
                        {problem.title}
                      </h3>
                      <p className="mt-3 text-sm leading-relaxed text-[var(--foreground-secondary)]">
                        {problem.description}
                      </p>
                    </div>
                  </FadeIn>
                ))}
              </div>
            </section>

            {/* Use cases / industries */}
            <section className="py-16 lg:py-24">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  Use cases
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                  Who uses Cerul Enterprise
                </h2>
                <p className="mx-auto mt-4 max-w-2xl text-[var(--foreground-secondary)]">
                  Any organization with a serious video corpus and a real need to query it.
                </p>
              </FadeIn>

              <div className="mt-12 grid gap-6 md:grid-cols-2 lg:grid-cols-3">
                {useCases.map((useCase, index) => (
                  <FadeIn key={useCase.industry} delay={index * 80}>
                    <div className="h-full rounded-3xl border border-[var(--border)] bg-[var(--surface)] p-7 transition hover:border-[var(--border-strong)]">
                      <h3 className="text-lg font-semibold text-[var(--foreground)]">
                        {useCase.industry}
                      </h3>
                      <p className="mt-1 text-xs text-[var(--foreground-tertiary)]">
                        {useCase.audience}
                      </p>
                      <ul className="mt-5 space-y-2.5">
                        {useCase.examples.map((example) => (
                          <li
                            key={example}
                            className="flex items-start gap-2 text-sm text-[var(--foreground-secondary)]"
                          >
                            <svg
                              className="mt-1 h-3 w-3 flex-shrink-0 text-[var(--brand-bright)]"
                              fill="currentColor"
                              viewBox="0 0 8 8"
                            >
                              <circle cx="4" cy="4" r="3" />
                            </svg>
                            <span className="italic">&ldquo;{example}&rdquo;</span>
                          </li>
                        ))}
                      </ul>
                    </div>
                  </FadeIn>
                ))}
              </div>
            </section>

            {/* Capabilities */}
            <section className="py-16 lg:py-24">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  Capabilities
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                  What Enterprise gives you
                </h2>
                <p className="mx-auto mt-4 max-w-2xl text-[var(--foreground-secondary)]">
                  Production capabilities beyond what we ship in our hosted public API.
                </p>
              </FadeIn>

              <div className="mt-12 grid gap-5 md:grid-cols-2 lg:grid-cols-4">
                {capabilities.map((capability, index) => (
                  <FadeIn key={capability.title} delay={index * 60}>
                    <div className="h-full rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-6 transition hover:border-[var(--border-strong)]">
                      <h3 className="text-sm font-semibold text-[var(--foreground)]">
                        {capability.title}
                      </h3>
                      <p className="mt-2 text-xs leading-relaxed text-[var(--foreground-secondary)]">
                        {capability.description}
                      </p>
                    </div>
                  </FadeIn>
                ))}
              </div>
            </section>

            {/* Deployment options */}
            <section id="deployment" className="scroll-mt-24 py-16 lg:py-24">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  Deployment
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                  Three ways to deploy
                </h2>
                <p className="mx-auto mt-4 max-w-2xl text-[var(--foreground-secondary)]">
                  Run Cerul where your data needs to live. We meet you where you are.
                </p>
              </FadeIn>

              <div className="mt-12 grid gap-6 lg:grid-cols-3">
                {deploymentOptions.map((option, index) => (
                  <FadeIn key={option.name} delay={index * 100}>
                    <div
                      className={`relative flex h-full flex-col rounded-3xl border p-8 transition-all duration-300 hover:shadow-lg ${
                        option.popular
                          ? "border-[var(--border-brand)] bg-gradient-to-b from-[var(--surface)] to-[var(--background-elevated)] shadow-[0_0_40px_-10px_rgba(34,211,238,0.15)] lg:scale-105"
                          : "border-[var(--border)] bg-[var(--surface)]"
                      }`}
                    >
                      {option.popular && (
                        <div className="absolute -top-4 left-1/2 -translate-x-1/2 rounded-full bg-gradient-to-r from-[var(--brand)] to-[var(--brand-bright)] px-4 py-1.5 text-xs font-semibold text-white shadow-lg">
                          Most chosen
                        </div>
                      )}

                      <div className="mb-6">
                        <p className="text-sm font-medium uppercase tracking-wider text-[var(--brand-bright)]">
                          {option.name}
                        </p>
                        <p className="mt-2 text-2xl font-bold tracking-tight text-[var(--foreground)]">
                          {option.tagline}
                        </p>
                        <p className="mt-4 text-sm leading-relaxed text-[var(--foreground-secondary)]">
                          {option.description}
                        </p>
                      </div>

                      <div className="mb-5 rounded-2xl border border-[var(--border)] bg-[var(--background-elevated)] p-4">
                        <p className="text-xs font-semibold uppercase tracking-wider text-[var(--foreground-tertiary)]">
                          Best for
                        </p>
                        <p className="mt-1 text-sm text-[var(--foreground)]">{option.bestFor}</p>
                      </div>

                      <ul className="mb-8 flex-1 space-y-3">
                        {option.features.map((feature) => (
                          <li
                            key={feature}
                            className="flex items-start gap-3 text-sm text-[var(--foreground-secondary)]"
                          >
                            <svg
                              className="mt-0.5 h-5 w-5 flex-shrink-0 text-[var(--success)]"
                              fill="none"
                              viewBox="0 0 24 24"
                              stroke="currentColor"
                            >
                              <path
                                strokeLinecap="round"
                                strokeLinejoin="round"
                                strokeWidth={2}
                                d="M5 13l4 4L19 7"
                              />
                            </svg>
                            {feature}
                          </li>
                        ))}
                      </ul>

                      <a
                        href="mailto:panda@cerul.ai?subject=Cerul%20Enterprise%20-%20Deployment%20Question"
                        className={`w-full text-center ${
                          option.popular ? "button-gradient" : "button-secondary"
                        }`}
                      >
                        Talk to sales
                      </a>
                    </div>
                  </FadeIn>
                ))}
              </div>
            </section>

            {/* Security & compliance */}
            <section className="py-16 lg:py-24">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  Security & Compliance
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                  Built for enterprise security review
                </h2>
                <p className="mx-auto mt-4 max-w-2xl text-[var(--foreground-secondary)]">
                  Your data never leaves your control. Standard controls, standard integrations.
                </p>
              </FadeIn>

              {/* Flagship no-training commitment */}
              <FadeIn delay={50}>
                <div className="relative mt-12 overflow-hidden rounded-[28px] border-2 border-[var(--border-brand)] bg-gradient-to-br from-[var(--brand-subtle)] via-[var(--surface)] to-[var(--background-elevated)] p-8 lg:p-10">
                  <div className="absolute -right-20 -top-20 h-48 w-48 rounded-full bg-[var(--brand)] opacity-15 blur-3xl" />
                  <div className="relative grid gap-6 lg:grid-cols-[auto_1fr_auto] lg:items-center">
                    <div className="flex h-14 w-14 flex-shrink-0 items-center justify-center rounded-2xl border border-[var(--border-brand)] bg-[var(--brand-subtle)] text-[var(--brand-bright)]">
                      <svg width="24" height="24" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                        <path strokeLinecap="round" strokeLinejoin="round" d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                      </svg>
                    </div>
                    <div>
                      <p className="text-xs font-semibold uppercase tracking-wider text-[var(--brand-bright)]">
                        Our foundational promise
                      </p>
                      <h3 className="mt-2 text-2xl font-bold tracking-tight text-[var(--foreground)] lg:text-3xl">
                        Your data never trains any model.
                      </h3>
                      <p className="mt-3 text-sm leading-relaxed text-[var(--foreground-secondary)] lg:text-base">
                        Customer videos, transcripts, embeddings, and queries are never used to
                        train any Cerul model or third-party model. We do not share customer data
                        with model providers for training. This is binding — written into our DPA.
                      </p>
                    </div>
                    <Link
                      href="/security"
                      className="button-secondary inline-flex items-center gap-2 self-start lg:self-center"
                    >
                      Read our security posture
                      <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                        <path strokeLinecap="round" strokeLinejoin="round" d="M9 5l7 7-7 7" />
                      </svg>
                    </Link>
                  </div>
                </div>
              </FadeIn>

              <div className="mt-10 grid gap-5 md:grid-cols-2 lg:grid-cols-3">
                {securityFeatures.map((feature, index) => (
                  <FadeIn key={feature.title} delay={index * 60}>
                    <div className="flex items-start gap-4 rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-5">
                      <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-xl border border-[var(--border-brand)] bg-[var(--brand-subtle)] text-[var(--brand-bright)]">
                        <svg
                          width="18"
                          height="18"
                          fill="none"
                          viewBox="0 0 24 24"
                          stroke="currentColor"
                          strokeWidth={2}
                        >
                          <path
                            strokeLinecap="round"
                            strokeLinejoin="round"
                            d="M9 12.75L11.25 15 15 9.75M21 12c0 1.268-.63 2.39-1.593 3.068a3.745 3.745 0 01-1.043 3.296 3.745 3.745 0 01-3.296 1.043A3.745 3.745 0 0112 21c-1.268 0-2.39-.63-3.068-1.593a3.746 3.746 0 01-3.296-1.043 3.745 3.745 0 01-1.043-3.296A3.745 3.745 0 013 12c0-1.268.63-2.39 1.593-3.068a3.745 3.745 0 011.043-3.296 3.746 3.746 0 013.296-1.043A3.746 3.746 0 0112 3c1.268 0 2.39.63 3.068 1.593a3.746 3.746 0 013.296 1.043 3.746 3.746 0 011.043 3.296A3.745 3.745 0 0121 12z"
                          />
                        </svg>
                      </div>
                      <div>
                        <h3 className="text-sm font-semibold text-[var(--foreground)]">
                          {feature.title}
                        </h3>
                        <p className="mt-1 text-xs leading-relaxed text-[var(--foreground-secondary)]">
                          {feature.description}
                        </p>
                      </div>
                    </div>
                  </FadeIn>
                ))}
              </div>
            </section>

            {/* Build vs Buy */}
            <section className="py-16 lg:py-24">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  Build vs. Buy
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                  The TCO is not close
                </h2>
                <p className="mx-auto mt-4 max-w-2xl text-[var(--foreground-secondary)]">
                  We&apos;ve been building video search in production for years. Most companies
                  underestimate the cost of doing it themselves by an order of magnitude.
                </p>
              </FadeIn>

              <FadeIn delay={100}>
                <div className="mt-12 overflow-hidden rounded-3xl border border-[var(--border)] bg-[var(--surface)]">
                  <div className="overflow-x-auto">
                    <table className="w-full">
                      <thead>
                        <tr className="border-b border-[var(--border)] bg-[var(--background-elevated)]">
                          <th className="px-6 py-4 text-left text-sm font-semibold text-[var(--foreground)]">
                            Dimension
                          </th>
                          <th className="px-6 py-4 text-left text-sm font-semibold text-[var(--foreground-secondary)]">
                            Build it yourself
                          </th>
                          <th className="px-6 py-4 text-left text-sm font-semibold text-[var(--brand-bright)]">
                            Cerul Enterprise
                          </th>
                        </tr>
                      </thead>
                      <tbody>
                        {buildVsBuy.map((row, index) => (
                          <tr
                            key={row.category}
                            className={
                              index !== buildVsBuy.length - 1 ? "border-b border-[var(--border)]" : ""
                            }
                          >
                            <td className="px-6 py-4 text-sm font-medium text-[var(--foreground)]">
                              {row.category}
                            </td>
                            <td className="px-6 py-4 text-sm text-[var(--foreground-secondary)]">
                              {row.build}
                            </td>
                            <td className="px-6 py-4 text-sm font-medium text-[var(--foreground)]">
                              {row.buy}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>
              </FadeIn>
            </section>

            {/* Process */}
            <section className="py-16 lg:py-24">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  Process
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                  How we work with you
                </h2>
              </FadeIn>

              <div className="mt-12 grid gap-6 md:grid-cols-2 lg:grid-cols-4">
                {process.map((step, index) => (
                  <FadeIn key={step.step} delay={index * 100}>
                    <div className="relative h-full rounded-3xl border border-[var(--border)] bg-[var(--surface)] p-7">
                      <div className="font-mono text-xs font-semibold uppercase tracking-wider text-[var(--brand-bright)]">
                        {step.step}
                      </div>
                      <h3 className="mt-4 text-lg font-semibold text-[var(--foreground)]">
                        {step.title}
                      </h3>
                      <p className="mt-3 text-sm leading-relaxed text-[var(--foreground-secondary)]">
                        {step.description}
                      </p>
                    </div>
                  </FadeIn>
                ))}
              </div>
            </section>

            {/* Open source bridge */}
            <section className="py-16 lg:py-24">
              <FadeIn>
                <div className="relative overflow-hidden rounded-[32px] border border-[var(--border-brand)] bg-gradient-to-br from-[var(--brand-subtle)] via-[var(--surface)] to-[var(--background-elevated)] p-10 lg:p-14">
                  <div className="absolute -right-32 -top-32 h-64 w-64 rounded-full bg-[var(--brand)] opacity-10 blur-3xl" />
                  <div className="relative grid gap-8 lg:grid-cols-[1.4fr,1fr] lg:items-center">
                    <div>
                      <span className="eyebrow text-[var(--brand-bright)]">Open source</span>
                      <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                        Try Cerul self-hosted first
                      </h2>
                      <p className="mt-4 max-w-xl text-[var(--foreground-secondary)]">
                        Cerul App is our open-source, self-hostable video search daemon. Same
                        pipeline, same retrieval architecture — without the tuned reranker, custom
                        models, and enterprise support. A great way to evaluate fit before talking
                        to sales.
                      </p>
                      <div className="mt-6 flex flex-wrap gap-3">
                        <a
                          href="https://github.com/cerul-ai/cerul-app"
                          target="_blank"
                          rel="noopener noreferrer"
                          className="button-secondary inline-flex items-center gap-2"
                        >
                          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
                            <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.765-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3-.405 1.02.005 2.047.138 3.006.404 2.295-1.56 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.23 1.905 1.23 3.225 0 4.609-2.807 5.625-5.475 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
                          </svg>
                          Cerul App on GitHub
                        </a>
                        <a
                          href="mailto:panda@cerul.ai?subject=Cerul%20Enterprise%20Demo%20Request"
                          className="button-gradient"
                        >
                          Talk to enterprise sales
                        </a>
                      </div>
                    </div>
                    <div className="rounded-2xl border border-[var(--border)] bg-[#1a1614] p-5 font-mono text-sm text-[#e8e4dc]">
                      <p className="text-xs text-white/30">Get started with the OSS daemon</p>
                      <pre className="mt-3 overflow-x-auto text-xs leading-relaxed lg:text-sm">
                        <code>{`# Self-host on your machine
docker run -d \\
  -p 7777:7777 \\
  -v ./data:/data \\
  cerulai/cerul-app

# Open http://localhost:7777`}</code>
                      </pre>
                    </div>
                  </div>
                </div>
              </FadeIn>
            </section>

            {/* FAQ */}
            <section className="py-16 lg:py-24">
              <FadeIn className="text-center">
                <span className="eyebrow inline-flex items-center gap-2">
                  <span className="inline-block h-px w-4 bg-[var(--brand)]" />
                  FAQ
                </span>
                <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl">
                  Frequently asked questions
                </h2>
              </FadeIn>

              <FadeIn delay={100}>
                <div className="mt-12 mx-auto grid max-w-4xl gap-3">
                  {enterpriseFaqs.map((item) => (
                    <details
                      key={item.question}
                      className="group rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-6 transition hover:border-[var(--border-strong)]"
                    >
                      <summary className="flex cursor-pointer list-none items-start justify-between gap-4 text-left">
                        <h3 className="font-semibold text-[var(--foreground)]">
                          {item.question}
                        </h3>
                        <svg
                          className="mt-1 h-5 w-5 flex-shrink-0 text-[var(--foreground-tertiary)] transition-transform group-open:rotate-45"
                          fill="none"
                          viewBox="0 0 24 24"
                          stroke="currentColor"
                          strokeWidth={2}
                        >
                          <path strokeLinecap="round" strokeLinejoin="round" d="M12 4v16m8-8H4" />
                        </svg>
                      </summary>
                      <p className="mt-4 text-sm leading-relaxed text-[var(--foreground-secondary)]">
                        {item.answer}
                      </p>
                    </details>
                  ))}
                </div>
              </FadeIn>
            </section>

            {/* Final CTA */}
            <section className="py-16 lg:py-24">
              <FadeIn>
                <div className="relative overflow-hidden rounded-[32px] border border-[var(--border)] bg-gradient-to-br from-[var(--surface)] via-[var(--background)] to-[var(--surface)] p-12 text-center lg:p-16">
                  <div className="absolute inset-0 bg-[radial-gradient(circle_at_center,rgba(34,211,238,0.10),transparent_55%)]" />
                  <div className="relative">
                    <h2 className="text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl lg:text-5xl">
                      Ready to make your video archive{" "}
                      <span className="text-[var(--brand-bright)]">searchable?</span>
                    </h2>
                    <p className="mx-auto mt-4 max-w-2xl text-lg text-[var(--foreground-secondary)]">
                      30-minute discovery call. No slide deck. We learn your corpus, your use case,
                      your constraints — and tell you straight whether Cerul is a fit.
                    </p>
                    <div className="mt-8 flex flex-wrap items-center justify-center gap-4">
                      <a
                        href="mailto:panda@cerul.ai?subject=Cerul%20Enterprise%20Demo%20Request"
                        className="button-gradient"
                      >
                        Book a demo
                      </a>
                      <Link href="/docs" className="button-secondary">
                        Read the docs
                      </Link>
                    </div>
                    <p className="mt-6 text-xs text-[var(--foreground-tertiary)]">
                      Or write to{" "}
                      <a
                        href="mailto:panda@cerul.ai"
                        className="text-[var(--brand-bright)] hover:text-[var(--foreground)]"
                      >
                        panda@cerul.ai
                      </a>
                    </p>
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
