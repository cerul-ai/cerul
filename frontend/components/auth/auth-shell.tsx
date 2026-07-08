import type { ReactNode } from "react";
import { BrandMark } from "@/components/brand-mark";

type AuthShellProps = {
  heroTitle: string;
  heroDescription: string;
  children: ReactNode;
};

export function AuthShell({
  heroTitle,
  heroDescription,
  children,
}: AuthShellProps) {
  return (
    <div className="soft-theme auth-theme min-h-screen">
      <div className="relative isolate mx-auto grid min-h-screen overflow-hidden lg:grid-cols-[minmax(0,1.08fr)_minmax(440px,0.92fr)]">
        <div className="pointer-events-none absolute inset-0 -z-10 bg-[radial-gradient(circle_at_12%_16%,rgba(180, 205, 230,0.52),transparent_24%),radial-gradient(circle_at_88%_12%,rgba(226, 231, 238,0.4),transparent_22%),linear-gradient(180deg,#f7f9fb_0%,#f7f0e3_52%,#f3eadc_100%)]" />
        <div className="pointer-events-none absolute inset-0 -z-10 opacity-60 [background-image:linear-gradient(rgba(58, 66, 80,0.035)_1px,transparent_1px),linear-gradient(90deg,rgba(58, 66, 80,0.035)_1px,transparent_1px)] [background-size:64px_64px]" />

        <section className="relative hidden border-r border-[var(--border)] px-12 py-10 lg:flex lg:flex-col lg:justify-between lg:px-16 xl:px-20">
          <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_28%_24%,rgba(255,255,255,0.52),transparent_22%),radial-gradient(circle_at_74%_68%,rgba(180, 205, 230,0.18),transparent_28%)]" />

          <div className="relative z-10">
            <BrandMark />
          </div>

          <div className="relative z-10 max-w-[560px]">
            <h1 className="display-title text-5xl leading-[1.3] tracking-[-0.05em] xl:text-6xl">
              {heroTitle}
            </h1>

            <p className="mt-6 max-w-[520px] text-base leading-8 text-[var(--foreground-secondary)]">
              {heroDescription}
            </p>
          </div>

          <div className="relative z-10 max-w-[420px]">
            <blockquote className="border-l-2 border-[var(--brand-bright)] pl-5">
              <p className="text-base leading-7 text-[var(--foreground-secondary)]">
                &ldquo;If an agent can read text, it should be able to search video.&rdquo;
              </p>
              <p className="mt-2 text-xs text-[var(--foreground-tertiary)]">— Cerul</p>
            </blockquote>
          </div>
        </section>

        <section className="relative flex min-h-screen items-center justify-center px-6 py-10 sm:px-8 lg:px-12">
          <div className="w-full max-w-[460px]">
            <div className="mb-6 lg:hidden">
              <BrandMark />
              <h1 className="mt-6 text-3xl font-semibold tracking-[-0.05em] text-[var(--foreground)]">
                {heroTitle}
              </h1>
              <p className="mt-3 text-sm leading-7 text-[var(--foreground-secondary)]">
                {heroDescription}
              </p>
            </div>

            <div className="relative">
              <div className="pointer-events-none absolute -inset-5 rounded-[36px] bg-[radial-gradient(circle_at_top,rgba(180, 205, 230,0.34),transparent_55%),radial-gradient(circle_at_bottom,rgba(226, 231, 238,0.28),transparent_48%)] blur-2xl" />
              <div className="surface-elevated relative rounded-[32px] px-8 py-8 shadow-[0_30px_70px_rgba(28, 38, 56,0.1)] sm:px-10 sm:py-10">
                {children}
              </div>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
