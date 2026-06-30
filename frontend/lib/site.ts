export const primaryNavigation = [
  { label: "Home", href: "/" },
  { label: "Docs", href: "/docs" },
  { label: "Pricing", href: "/pricing" },
  { label: "Enterprise", href: "/enterprise" },
  { label: "App", href: "https://app.cerul.ai", external: true },
  { label: "Dashboard", href: "/dashboard" },
] as const;

export const ACCOUNT_SETTINGS_ROUTE = "/dashboard/settings#account" as const;

export const marketingMetrics = [
  {
    label: "Visual search",
    value: "See what's on screen",
    caption:
      "Slides, charts, demos, and whiteboard sketches become searchable context for your agent.",
  },
  {
    label: "Speech + transcript",
    value: "Hear what's said",
    caption:
      "Every spoken word is indexed and aligned with the visual timeline, so you can search both.",
  },
  {
    label: "One API call",
    value: "Built for agents",
    caption:
      "A single endpoint returns grounded results with timestamps, relevance scores, and source links.",
  },
] as const;

export const capabilityHighlights = [
  {
    kicker: "Visual retrieval",
    title: "Ground on-screen evidence",
    description:
      "Slides, charts, product demos, whiteboards, and screen recordings become searchable context for downstream reasoning.",
  },
  {
    kicker: "Thin orchestration",
    title: "Keep the API layer narrow",
    description:
      "Hono on Cloudflare Workers handles auth, usage, and response shaping. Indexing and other media-heavy work stay in Python workers.",
  },
  {
    kicker: "Replaceable models",
    title: "Do not lock product logic to one model",
    description:
      "CLIP, OpenAI embeddings, or future internal models can slot into the same shared retrieval backbone.",
  },
] as const;

export const benchmarkRows = [
  {
    label: "Slide recall",
    description: "How well the system captures text and chart evidence from frames",
    score: 93,
  },
  {
    label: "Demo grounding",
    description: "Whether product screens and on-screen actions remain queryable",
    score: 88,
  },
  {
    label: "Transcript-only gap",
    description: "How much signal is lost if the system reads words but ignores visuals",
    score: 71,
  },
] as const;

export const dashboardSignals = [
  {
    label: "Usage ledger",
    value: "128 credits",
    change: "+16 today",
    caption: "Monthly credit accounting with the same request IDs used for search logs.",
  },
  {
    label: "Index freshness",
    value: "3h 14m",
    change: "Within target",
    caption: "Current lag between source discovery and available search results.",
  },
  {
    label: "Search health",
    value: "99.94%",
    change: "Stable",
    caption: "Healthy request success rate across the shared retrieval surface.",
  },
] as const;

export const demoModes = {
  search: {
    label: "search",
    query: "Find the segment where the speaker explains the AGI timeline and shows a roadmap slide.",
    tags: ["timestamp url", "speaker filters", "answer optional"],
    surface: "Unified retrieval",
    output: "Summary plus time range",
  },
  visual: {
    label: "visual",
    query: "Cinematic close-up of a robotic arm sorting packages in a bright warehouse.",
    tags: ["preview image", "duration filters", "source metadata"],
    surface: "Visual retrieval",
    output: "Thumbnail, preview, duration",
  },
  agent: {
    label: "agent skill",
    query: "Return sources and timestamps I can cite in an autonomous research brief about AI chip supply constraints.",
    tags: ["citations", "api keys", "direct http"],
    surface: "Skill integration",
    output: "Structured references for downstream agents",
  },
} as const;

export const pricingTiers = [
  {
    name: "Free",
    price: "$0",
    cadence: "to get started",
    description:
      "100 free credits to try the API, plus 10 free searches every day.",
    checkoutProductCode: null,
    ctaLabel: "Start free",
    ctaHref: "/login?mode=signup",
    accent: "sky",
    features: [
      "100 credits on signup",
      "10 free searches every day",
      "Full public search API access",
      "Community support",
    ],
  },
  {
    name: "Pay as you go",
    price: "$8",
    cadence: "per 1,000 credits",
    description:
      "Buy credits when you need them. No subscription, no commitment.",
    checkoutProductCode: "topup",
    ctaLabel: "Buy 1,000 credits",
    ctaHref: "/login?mode=signup",
    accent: "orange",
    features: [
      "No monthly fee",
      "Min 1,000 credits per purchase",
      "Auto-recharge available",
      "10 free searches every day",
    ],
  },
  {
    name: "Pro",
    price: "$29.90",
    cadence: "per month",
    description:
      "5,000 included credits every month. Top up at $8/1K when you need more.",
    checkoutProductCode: "pro",
    ctaLabel: "Subscribe",
    ctaHref: "/login?mode=signup",
    accent: "blue",
    features: [
      "5,000 included credits / month",
      "Top up at $8/1K for more",
      "Higher rate limits",
      "10 free searches every day",
      "Priority email support",
    ],
  },
  {
    name: "Enterprise",
    price: "Custom",
    cadence: "let\u2019s talk",
    description:
      "For production deployments with private indexing, SLA expectations, and compliance review.",
    checkoutProductCode: null,
    ctaLabel: "Talk to us",
    ctaHref: "mailto:support@cerul.ai",
    accent: "ink",
    features: [
      "Custom volume pricing",
      "Private indexing workflows",
      "Dedicated onboarding and review",
      "Enterprise security and SLAs",
    ],
  },
] as const;

export const pricingFaqs = [
  {
    question: "How do credits work?",
    answer:
      "A standard search costs 1 credit. A search with include_answer=true costs 2 credits. Every user gets 10 free searches per day — no credits needed.",
  },
  {
    question: "What happens when credits run out?",
    answer:
      "Requests beyond the 10 daily free searches will be blocked until you buy more credits or enable auto-recharge.",
  },
  {
    question: "What is auto-recharge?",
    answer:
      "When enabled, we automatically charge your card and add credits when your balance drops below a threshold you set. You stay in control of the recharge amount.",
  },
  {
    question: "Why choose Pro over Pay as you go?",
    answer:
      "Pro gives you 5,000 credits per month at an effective rate of $5.98/1K — versus $8/1K for pay-as-you-go top-ups. You also get higher rate limits and priority support.",
  },
  {
    question: "When does enterprise make sense?",
    answer:
      "Once a team needs private indexing, custom rate limits, security review, or volume pricing, reach out and we\u2019ll put together a plan that fits.",
  },
] as const;

export const authValueProps = [
  {
    title: "Shared platform visibility",
    description:
      "Track the same request IDs across public API calls, dashboard usage views, and future worker logs.",
  },
  {
    title: "Thin auth surface",
    description:
      "Web auth remains separate from API key auth, keeping the public integration path simple.",
  },
  {
    title: "Operator-first defaults",
    description:
      "The console is designed around key management, usage control, and pipeline health before deeper product chrome.",
  },
] as const;

export const dashboardRoutes = [
  { label: "Overview", href: "/dashboard", meta: "01" },
  { label: "API Playground", href: "/dashboard/playground", meta: "02" },
  { label: "Usage", href: "/dashboard/usage", meta: "03" },
  { label: "Billing", href: "/dashboard/billing", meta: "04" },
  { label: "Settings", href: "/dashboard/settings", meta: "05" },
  { label: "Query Logs", href: "/dashboard/query-logs", meta: "06" },
] as const;

export const adminRoutes = [
  { label: "Overview", href: "/admin", meta: "A1" },
  { label: "Analytics", href: "/admin/analytics", meta: "A2" },
  { label: "Requests", href: "/admin/requests", meta: "A3" },
  { label: "Query Logs", href: "/admin/query-logs", meta: "A4" },
  { label: "Workers", href: "/admin/workers", meta: "A5" },
  { label: "Sources", href: "/admin/sources", meta: "A6" },
  { label: "Content", href: "/admin/content", meta: "A7" },
] as const;

export function isPrimaryRoute(path: string): boolean {
  return primaryNavigation.some((item) => !("external" in item) && item.href === path);
}

export function isPrimaryNavigationActive(
  currentPath: string,
  itemHref: (typeof primaryNavigation)[number]["href"],
): boolean {
  if (itemHref.startsWith("http")) {
    return false;
  }

  if (itemHref === "/") {
    return currentPath === "/";
  }

  return currentPath === itemHref || currentPath.startsWith(`${itemHref}/`);
}

export function isDashboardRouteActive(currentPath: string, href: string): boolean {
  const pathname = currentPath.split(/[?#]/, 1)[0] ?? currentPath;
  if (href === "/dashboard") {
    return pathname === "/dashboard";
  }

  return pathname === href || pathname.startsWith(`${href}/`);
}

export function isAdminRouteActive(currentPath: string, href: string): boolean {
  const pathname = currentPath.split(/[?#]/, 1)[0] ?? currentPath;
  if (href === "/admin") {
    return pathname === "/admin";
  }

  return pathname === href || pathname.startsWith(`${href}/`);
}
