import type { Route } from "next";
import Link from "next/link";
import { FadeIn } from "@/components/animations";

interface FeaturesProps {
  features: Array<{
    icon: React.ReactNode;
    title: string;
    description: string;
    href?: string;
  }>;
  eyebrow?: string;
  title?: string;
  description?: string;
}

export function Features({
  features,
  eyebrow,
  title,
  description,
}: FeaturesProps) {
  return (
    <section className="py-16 lg:py-24">
      {(eyebrow || title || description) && (
        <FadeIn className="mb-12 text-center lg:mb-16">
          {eyebrow && (
            <span className="eyebrow inline-flex items-center gap-2">
              <span className="inline-block h-px w-4 bg-[var(--brand)]" />
              {eyebrow}
            </span>
          )}
          {title && (
            <h2 className="mt-4 text-3xl font-bold tracking-tight text-[var(--foreground)] sm:text-4xl lg:text-5xl">
              {title}
            </h2>
          )}
          {description && (
            <p className="mx-auto mt-4 max-w-2xl text-lg text-[var(--foreground-secondary)]">
              {description}
            </p>
          )}
        </FadeIn>
      )}

      <div className="grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
        {features.map((feature, index) => (
          <FadeIn key={feature.title} delay={index * 100}>
            <FeatureCard {...feature} />
          </FadeIn>
        ))}
      </div>
    </section>
  );
}

interface FeatureCardProps {
  icon: React.ReactNode;
  title: string;
  description: string;
  href?: string;
}

function FeatureCard({ icon, title, description, href }: FeatureCardProps) {
  const wrapperClassName = href
    ? "group block h-full rounded-3xl border border-[var(--border)] bg-[var(--surface)] p-8 transition-all duration-300 hover:border-[var(--border-brand)] hover:bg-[var(--surface-hover)] hover:shadow-[0_0_40px_-10px_rgba(111, 151, 201,0.15)]"
    : "group h-full rounded-3xl border border-[var(--border)] bg-[var(--surface)] p-8 transition-all duration-300 hover:border-[var(--border-strong)] hover:bg-[var(--surface-hover)]";

  const content = (
    <>
      <div className="mb-5 inline-flex h-14 w-14 items-center justify-center rounded-2xl bg-gradient-to-br from-[var(--brand-subtle)] to-transparent text-[var(--brand-bright)] ring-1 ring-[var(--border-brand)] transition-transform duration-300 group-hover:scale-110">
        {icon}
      </div>
      <h3 className="text-xl font-semibold text-[var(--foreground)]">
        {title}
      </h3>
      <p className="mt-3 text-[15px] leading-relaxed text-[var(--foreground-secondary)]">
        {description}
      </p>
      {href ? (
        <div className="mt-5 flex items-center gap-2 text-sm font-medium text-[var(--brand-bright)] transition-colors group-hover:text-[var(--foreground)]">
          Learn more
          <svg
            className="h-4 w-4 transition-transform duration-300 group-hover:translate-x-1"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M17 8l4 4m0 0l-4 4m4-4H3"
            />
          </svg>
        </div>
      ) : null}
    </>
  );

  if (href) {
    return (
      <Link href={href as Route} className={wrapperClassName}>
        {content}
      </Link>
    );
  }

  return <div className={wrapperClassName}>{content}</div>;
}

interface FeatureGridProps {
  features: Array<{
    icon: React.ReactNode;
    title: string;
    description: string;
  }>;
}

export function FeatureGrid({ features }: FeatureGridProps) {
  return (
    <div className="grid gap-px overflow-hidden rounded-[28px] bg-[var(--border)] sm:grid-cols-2 lg:grid-cols-4">
      {features.map((feature) => (
        <div
          key={feature.title}
          className="group bg-[var(--background)] p-8 transition-colors duration-300 hover:bg-[var(--surface)]"
        >
          <div className="mb-4 inline-flex h-12 w-12 items-center justify-center rounded-xl bg-[var(--surface)] text-[var(--brand-bright)] ring-1 ring-[var(--border)] transition-all duration-300 group-hover:scale-110 group-hover:bg-[var(--brand-subtle)] group-hover:ring-[var(--border-brand)]">
            {feature.icon}
          </div>
          <h3 className="text-lg font-semibold text-[var(--foreground)]">
            {feature.title}
          </h3>
          <p className="mt-2 text-sm leading-relaxed text-[var(--foreground-secondary)]">
            {feature.description}
          </p>
        </div>
      ))}
    </div>
  );
}
