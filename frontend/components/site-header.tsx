import Link from "next/link";
import { BrandMark } from "@/components/brand-mark";
import { SiteHeaderAuthActions } from "@/components/site-header-auth-actions";
import { isPrimaryNavigationActive, primaryNavigation } from "@/lib/site";

interface SiteHeaderProps {
  currentPath: string;
}

export function SiteHeader({ currentPath }: SiteHeaderProps) {
  const visibleNavigation = primaryNavigation.filter((item) => item.href !== "/dashboard");

  return (
    <header className="sticky top-4 z-50 mx-auto max-w-[1400px]">
      <div className="surface-elevated flex items-center justify-between gap-3 rounded-[28px] px-3 py-3 sm:rounded-full sm:px-2 sm:py-2 sm:pr-4 lg:pr-6 backdrop-blur-xl">
        {/* Logo */}
        <div className="min-w-0 flex items-center pl-2 sm:pl-4 lg:pl-6">
          <BrandMark />
        </div>

        {/* Navigation - centered */}
        <nav className="hidden items-center gap-1 lg:flex lg:mx-6">
          {visibleNavigation.map((item) => {
            const isActive = isPrimaryNavigationActive(currentPath, item.href);
            const className = `relative rounded-full px-4 py-2 text-sm font-medium transition-all duration-200 ${
              isActive
                ? "text-[var(--foreground)]"
                : "text-[var(--foreground-secondary)] hover:text-[var(--foreground)]"
            }`;

            if ("external" in item) {
              return (
                <a
                  key={item.label}
                  href={item.href}
                  target="_blank"
                  rel="noreferrer"
                  className={className}
                >
                  <span className="relative z-10 inline-flex items-center gap-1.5">
                    {item.label}
                    <svg className="h-3.5 w-3.5" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                      <path d="M6 4h6v6M12 4 5 11" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
                    </svg>
                  </span>
                </a>
              );
            }

            return (
              <Link
                key={item.label}
                href={item.href}
                className={className}
              >
                {isActive && (
                  <span className="absolute inset-0 rounded-full bg-[var(--background-sunken)] shadow-[inset_0_1px_0_rgba(255,255,255,0.68)]" />
                )}
                <span className="relative z-10">{item.label}</span>
              </Link>
            );
          })}
        </nav>

        {/* Actions */}
        <div className="flex shrink-0 items-center gap-2 lg:gap-3">
          <a
            href="https://github.com/cerul-ai/cerul"
            target="_blank"
            rel="noreferrer"
            aria-label="Open Cerul on GitHub"
            className="focus-ring inline-flex h-10 w-10 items-center justify-center rounded-full border border-[var(--border)] bg-white/70 text-[var(--foreground-secondary)] transition hover:border-[var(--border-strong)] hover:bg-white hover:text-[var(--foreground)] lg:hidden"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.765-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3-.405 1.02.005 2.047.138 3.006.404 2.295-1.56 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.23 1.905 1.23 3.225 0 4.609-2.807 5.625-5.475 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
            </svg>
          </a>
          <a
            href="https://github.com/cerul-ai/cerul"
            target="_blank"
            rel="noreferrer"
            className="focus-ring hidden h-10 items-center gap-2 rounded-full border border-[var(--border)] bg-white/70 px-4 text-sm font-medium text-[var(--foreground-secondary)] transition hover:border-[var(--border-strong)] hover:bg-white hover:text-[var(--foreground)] lg:inline-flex"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.765-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3-.405 1.02.005 2.047.138 3.006.404 2.295-1.56 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.23 1.905 1.23 3.225 0 4.609-2.807 5.625-5.475 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
            </svg>
            GitHub
          </a>
          <SiteHeaderAuthActions currentPath={currentPath} />
        </div>
      </div>

      {/* Mobile navigation */}
      <nav className="mt-3 flex items-center justify-start gap-1 overflow-x-auto rounded-full bg-[var(--surface)] p-1 backdrop-blur-xl [scrollbar-width:none] [&::-webkit-scrollbar]:hidden lg:hidden">
        {visibleNavigation.map((item) => {
          const isActive = isPrimaryNavigationActive(currentPath, item.href);
          const className = `rounded-full px-3 py-1.5 text-xs font-medium transition-all ${
            isActive
              ? "bg-[var(--background-sunken)] text-[var(--foreground)] shadow-[inset_0_1px_0_rgba(255,255,255,0.68)]"
              : "text-[var(--foreground-secondary)]"
          }`;

          if ("external" in item) {
            return (
              <a
                key={item.label}
                href={item.href}
                target="_blank"
                rel="noreferrer"
                className={className}
              >
                {item.label}
              </a>
            );
          }

          return (
            <Link
              key={item.label}
              href={item.href}
              className={className}
            >
              {item.label}
            </Link>
          );
        })}
      </nav>
    </header>
  );
}
