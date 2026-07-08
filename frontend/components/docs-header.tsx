"use client";

import { useDeferredValue, useEffect, useRef, useState } from "react";
import Link from "next/link";
import { BrandMark } from "@/components/brand-mark";
import {
  docsShellTabs,
  getDocsSearchEntries,
} from "@/lib/docs";

type DocsHeaderProps = {
  currentPath: string;
};

function isDocsTabActive(currentPath: string, href: string): boolean {
  if (href === "/docs") {
    return currentPath === "/docs" || currentPath.startsWith("/docs/quickstart");
  }

  if (href === "/docs/api-reference") {
    return currentPath.startsWith("/docs/api-reference");
  }

  return currentPath === href || currentPath.startsWith(`${href}/`);
}

export function DocsHeader({ currentPath }: DocsHeaderProps) {
  const [searchOpen, setSearchOpen] = useState(false);
  const [query, setQuery] = useState("");
  const deferredQuery = useDeferredValue(query);
  const searchEntries = getDocsSearchEntries();
  const activeTabRef = useRef<HTMLAnchorElement | null>(null);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      const isShortcut = (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k";

      if (isShortcut) {
        event.preventDefault();
        setSearchOpen(true);
      }

      if (event.key === "Escape") {
        setSearchOpen(false);
      }
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, []);

  const normalizedQuery = deferredQuery.trim().toLowerCase();
  const filteredEntries = !normalizedQuery
    ? searchEntries.slice(0, 8)
    : searchEntries.filter((entry) => {
      const haystack = `${entry.title} ${entry.description} ${entry.category}`.toLowerCase();
      return haystack.includes(normalizedQuery);
    });

  useEffect(() => {
    activeTabRef.current?.scrollIntoView({
      block: "nearest",
      inline: "center",
      behavior: "smooth",
    });
  }, [currentPath]);

  return (
    <>
      <header className="sticky top-0 z-50 border-b border-[var(--border)] bg-[rgba(252,248,241,0.82)] backdrop-blur-xl">
        <div className="mx-auto max-w-[1520px] px-4 py-3 sm:px-6 lg:px-8">
          <div className="flex items-center gap-3">
            <div className="flex min-w-0 items-center gap-3">
              <BrandMark />
            </div>

            <div className="hidden min-w-0 flex-1 items-center gap-2 lg:flex">
              <button
                type="button"
                onClick={() => setSearchOpen(true)}
                className="flex h-10 min-w-0 flex-1 items-center justify-between rounded-full border border-[var(--border)] bg-white/72 px-4 text-sm text-[var(--foreground-secondary)] transition hover:border-[var(--border-strong)] hover:bg-white hover:text-[var(--foreground)]"
              >
                <span className="flex items-center gap-2">
                  <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path d="m21 21-4.35-4.35m1.85-5.15a7 7 0 1 1-14 0 7 7 0 0 1 14 0Z" strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} />
                  </svg>
                  Search docs...
                </span>
                <span className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                  ⌘K
                </span>
              </button>

              <button
                type="button"
                onClick={() => {
                  document
                    .querySelector("[data-docs-ai-anchor='true']")
                    ?.scrollIntoView({ behavior: "smooth", block: "center" });
                }}
                className="flex h-10 items-center justify-center rounded-full border border-[var(--border-brand)] bg-[var(--brand-subtle)] px-4 text-sm font-medium text-[var(--brand-bright)] transition hover:bg-[rgba(111, 151, 201,0.18)]"
              >
                Ask AI
              </button>
            </div>

            <div className="ml-auto flex items-center gap-2">
              <button
                type="button"
                onClick={() => setSearchOpen(true)}
                className="inline-flex h-10 items-center justify-center rounded-full border border-[var(--border)] bg-white/72 px-3 text-sm text-[var(--foreground-secondary)] transition hover:border-[var(--border-strong)] hover:bg-white hover:text-[var(--foreground)] lg:hidden"
              >
                Search
              </button>
              <a
                href="mailto:support@cerul.ai"
                className="hidden rounded-full px-3 py-2 text-sm text-[var(--foreground-tertiary)] transition hover:bg-white/40 hover:text-[var(--foreground)] sm:inline-flex"
              >
                Support
              </a>
              <Link href="/login?mode=signup" className="button-secondary min-h-10 px-4 py-2 text-sm">
                Get API key
              </Link>
            </div>
          </div>

          <nav className="mt-3 flex items-center gap-1 overflow-x-auto border-t border-[var(--border)] pt-3 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
            {docsShellTabs.map((item) => {
              const active = isDocsTabActive(currentPath, item.href);

              return (
                <Link
                  key={item.href}
                  href={item.href}
                  ref={active ? activeTabRef : undefined}
                  className={`whitespace-nowrap rounded-full px-4 py-2 text-sm font-medium transition ${
                    active
                      ? "bg-white text-[var(--foreground)] shadow-[0_10px_24px_rgba(36,29,21,0.06)]"
                      : "text-[var(--foreground-secondary)] hover:bg-white/60 hover:text-[var(--foreground)]"
                  }`}
                >
                  {item.label}
                </Link>
              );
            })}
          </nav>
        </div>
      </header>

      {searchOpen ? (
        <div className="fixed inset-0 z-[120] flex items-start justify-center px-4 py-16 sm:px-6">
          <div
            className="absolute inset-0 bg-[rgba(36,29,21,0.4)] backdrop-blur-sm"
            onClick={() => setSearchOpen(false)}
          />
          <div className="relative z-10 w-full max-w-2xl overflow-hidden rounded-[24px] border border-[var(--border)] bg-[var(--background-elevated)] shadow-[0_28px_80px_rgba(36,29,21,0.18)]">
            <div className="border-b border-[var(--border)] px-5 py-4">
              <input
                autoFocus
                type="text"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Search guides, endpoints, and topics..."
                className="h-12 w-full rounded-[18px] border border-[var(--border)] bg-white/80 px-4 text-sm text-[var(--foreground)] outline-none transition placeholder:text-[var(--foreground-tertiary)] focus:border-[var(--border-brand)]"
              />
            </div>

            <div className="max-h-[60vh] overflow-y-auto p-3">
              {filteredEntries.length > 0 ? (
                filteredEntries.map((entry) => (
                  <a
                    key={`${entry.href}-${entry.title}`}
                    href={entry.href}
                    onClick={() => {
                      setSearchOpen(false);
                      setQuery("");
                    }}
                    className="block rounded-[18px] border border-transparent px-4 py-4 transition hover:border-[var(--border)] hover:bg-white"
                  >
                    <div className="flex items-center justify-between gap-3">
                      <p className="text-base font-semibold text-[var(--foreground)]">
                        {entry.title}
                      </p>
                      <span className="rounded-full border border-[var(--border)] px-2.5 py-1 text-[11px] text-[var(--foreground-tertiary)]">
                        {entry.category}
                      </span>
                    </div>
                    <p className="mt-2 text-sm leading-6 text-[var(--foreground-secondary)]">
                      {entry.description}
                    </p>
                  </a>
                ))
              ) : (
                <div className="px-4 py-8 text-center">
                  <p className="text-sm text-[var(--foreground-secondary)]">
                    No matching documentation entries found.
                  </p>
                </div>
              )}
            </div>
          </div>
        </div>
      ) : null}
    </>
  );
}
