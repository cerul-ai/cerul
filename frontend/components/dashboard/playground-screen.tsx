"use client";

import { useEffect, useMemo, useState } from "react";
import {
  apiKeys,
  playground,
  getApiErrorMessage,
  type DashboardApiKey,
  type PlaygroundSearchResponse,
  type PlaygroundSearchResult,
} from "@/lib/api";
import { DashboardLayout } from "./dashboard-layout";

/* ── Types ───────────────────────────────────────── */

type RightPanel = "code" | "response";
type CodeLang = "python" | "javascript" | "shell" | "go";
type ResponseTab = "preview" | "json";

/* ── Helpers ─────────────────────────────────────── */

function maskKey(prefix: string): string {
  return `${prefix}${"*".repeat(40)}`;
}

function formatDuration(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

/* ── Code snippet builders ───────────────────────── */

function buildCodeSnippet(lang: CodeLang, query: string, authToken: string): string {
  const q = query || "your search query here";

  if (lang === "python") {
    return `# To install: pip install cerul
from cerul import Cerul

client = Cerul(api_key="${authToken}")

result = client.search(
    query=${JSON.stringify(q)},
    max_results=5,
)

for r in result:
    print(r.title, r.url)`;
  }

  if (lang === "javascript") {
    return `import { cerul } from "cerul";

const client = cerul({ apiKey: "${authToken}" });

const result = await client.search({
  query: ${JSON.stringify(q)},
  max_results: 5,
});

for (const r of result.results) {
  console.log(r.title, r.url);
}`;
  }

  if (lang === "go") {
    return `package main

import (
    "bytes"
    "encoding/json"
    "fmt"
    "io"
    "net/http"
)

func main() {
    body, _ := json.Marshal(map[string]interface{}{
        "query":       ${JSON.stringify(q)},
        "max_results": 5,
    })
    req, _ := http.NewRequest(
        "POST",
        "https://api.cerul.ai/v1/search",
        bytes.NewBuffer(body),
    )
    req.Header.Set("Authorization", "Bearer ${authToken}")
    req.Header.Set("Content-Type", "application/json")

    resp, _ := http.DefaultClient.Do(req)
    defer resp.Body.Close()
    data, _ := io.ReadAll(resp.Body)
    fmt.Println(string(data))
}`;
  }

  // shell / curl
  return `curl -X POST https://api.cerul.ai/v1/search \\
  -H "Authorization: Bearer ${authToken}" \\
  -H "Content-Type: application/json" \\
  -d '{
    "query": ${JSON.stringify(q)},
    "max_results": 5
  }'`;
}

/* ── Syntax coloring (simple token-based) ────────── */

type TokenSpan = { text: string; color: string };

function tokenizeCode(code: string, lang: CodeLang): TokenSpan[] {
  const spans: TokenSpan[] = [];
  const lines = code.split("\n");

  for (let i = 0; i < lines.length; i++) {
    if (i > 0) spans.push({ text: "\n", color: "" });
    const line = lines[i];

    if (lang === "python") {
      if (line.trimStart().startsWith("#")) {
        spans.push({ text: line, color: "#6a9955" });
        continue;
      }
    }
    if (lang === "shell") {
      if (line.trimStart().startsWith("#")) {
        spans.push({ text: line, color: "#6a9955" });
        continue;
      }
    }

    // Simple token-based coloring
    let remaining = line;
    while (remaining.length > 0) {
      // String literals (double-quoted)
      const dqMatch = remaining.match(/^"(?:[^"\\]|\\.)*"/);
      if (dqMatch) {
        spans.push({ text: dqMatch[0], color: "#ce9178" });
        remaining = remaining.slice(dqMatch[0].length);
        continue;
      }
      // String literals (single-quoted)
      const sqMatch = remaining.match(/^'(?:[^'\\]|\\.)*'/);
      if (sqMatch) {
        spans.push({ text: sqMatch[0], color: "#ce9178" });
        remaining = remaining.slice(sqMatch[0].length);
        continue;
      }
      // Numbers
      const numMatch = remaining.match(/^\b\d+\.?\d*\b/);
      if (numMatch) {
        spans.push({ text: numMatch[0], color: "#b5cea8" });
        remaining = remaining.slice(numMatch[0].length);
        continue;
      }
      // Keywords
      const kwMatch = remaining.match(/^(?:import|from|def|class|return|const|let|var|await|async|function|package|func|main|defer|if|else|for|range|nil|null|true|false|True|False|None|print|fmt)\b/);
      if (kwMatch) {
        spans.push({ text: kwMatch[0], color: "#c586c0" });
        remaining = remaining.slice(kwMatch[0].length);
        continue;
      }
      // Curl flags
      if (lang === "shell") {
        const flagMatch = remaining.match(/^-[A-Za-z]+/);
        if (flagMatch) {
          spans.push({ text: flagMatch[0], color: "#569cd6" });
          remaining = remaining.slice(flagMatch[0].length);
          continue;
        }
      }
      // Default: take one char
      spans.push({ text: remaining[0], color: "" });
      remaining = remaining.slice(1);
    }
  }
  return spans;
}

function tokenizeJson(json: string): TokenSpan[] {
  const spans: TokenSpan[] = [];
  let remaining = json;

  while (remaining.length > 0) {
    // Property keys
    const keyMatch = remaining.match(/^"[^"]*"\s*:/);
    if (keyMatch) {
      const colonIdx = keyMatch[0].lastIndexOf(":");
      const key = keyMatch[0].slice(0, colonIdx);
      spans.push({ text: key, color: "#9cdcfe" });
      spans.push({ text: keyMatch[0].slice(colonIdx), color: "" });
      remaining = remaining.slice(keyMatch[0].length);
      continue;
    }
    // String values
    const strMatch = remaining.match(/^"(?:[^"\\]|\\.)*"/);
    if (strMatch) {
      spans.push({ text: strMatch[0], color: "#ce9178" });
      remaining = remaining.slice(strMatch[0].length);
      continue;
    }
    // Numbers
    const numMatch = remaining.match(/^-?\d+\.?\d*(?:[eE][+-]?\d+)?/);
    if (numMatch) {
      spans.push({ text: numMatch[0], color: "#b5cea8" });
      remaining = remaining.slice(numMatch[0].length);
      continue;
    }
    // Booleans and null
    const boolMatch = remaining.match(/^(?:true|false|null)\b/);
    if (boolMatch) {
      spans.push({ text: boolMatch[0], color: "#569cd6" });
      remaining = remaining.slice(boolMatch[0].length);
      continue;
    }
    spans.push({ text: remaining[0], color: "" });
    remaining = remaining.slice(1);
  }
  return spans;
}

/* ── Icons ───────────────────────────────────────── */

function IconResponse({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={1.5}>
      <path strokeLinecap="round" strokeLinejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 0 0-3.375-3.375h-1.5A1.125 1.125 0 0 1 13.5 7.125v-1.5a3.375 3.375 0 0 0-3.375-3.375H8.25m0 12.75h7.5m-7.5 3H12M10.5 2.25H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 0 0-9-9Z" />
    </svg>
  );
}

function IconCode({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={1.5}>
      <path strokeLinecap="round" strokeLinejoin="round" d="M17.25 6.75 22.5 12l-5.25 5.25m-10.5 0L1.5 12l5.25-5.25m7.5-3-4.5 16.5" />
    </svg>
  );
}

function IconCopy({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={1.5}>
      <path strokeLinecap="round" strokeLinejoin="round" d="M15.666 3.888A2.25 2.25 0 0 0 13.5 2.25h-3c-1.03 0-1.9.693-2.166 1.638m7.332 0c.055.194.084.4.084.612v0a.75.75 0 0 1-.75.75H9.334a.75.75 0 0 1-.75-.75v0c0-.212.03-.418.084-.612m7.332 0c.646.049 1.288.11 1.927.184 1.1.128 1.907 1.077 1.907 2.185V19.5a2.25 2.25 0 0 1-2.25 2.25H6.75A2.25 2.25 0 0 1 4.5 19.5V6.257c0-1.108.806-2.057 1.907-2.185a48.208 48.208 0 0 1 1.927-.184" />
    </svg>
  );
}

function IconCheck({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={2}>
      <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
    </svg>
  );
}

function IconThumbUp({ className, filled }: { className?: string; filled?: boolean }) {
  return (
    <svg className={className} fill={filled ? "currentColor" : "none"} stroke="currentColor" viewBox="0 0 24 24" strokeWidth={1.5}>
      <path strokeLinecap="round" strokeLinejoin="round" d="M6.633 10.25c.806 0 1.533-.446 2.031-1.08a9.041 9.041 0 0 1 2.861-2.4c.723-.384 1.35-.956 1.653-1.715a4.498 4.498 0 0 0 .322-1.672V2.75a.75.75 0 0 1 .75-.75 2.25 2.25 0 0 1 2.25 2.25c0 1.152-.26 2.243-.723 3.218-.266.558.107 1.282.725 1.282m0 0h3.126c1.026 0 1.945.694 2.054 1.715.045.422.068.85.068 1.285a11.95 11.95 0 0 1-2.649 7.521c-.388.482-.987.729-1.605.729H13.48c-.483 0-.964-.078-1.423-.23l-3.114-1.04a4.501 4.501 0 0 0-1.423-.23H5.904m10.598-9.75H14.25M5.904 18.5c.083.205.173.405.27.602.197.4-.078.898-.523.898h-.908c-.889 0-1.713-.518-1.972-1.368a12 12 0 0 1-.521-3.507c0-1.553.295-3.036.831-4.398C3.387 10.095 4.167 9.5 5.032 9.5h.876" />
    </svg>
  );
}

function IconThumbDown({ className, filled }: { className?: string; filled?: boolean }) {
  return (
    <svg className={className} fill={filled ? "currentColor" : "none"} stroke="currentColor" viewBox="0 0 24 24" strokeWidth={1.5}>
      <path strokeLinecap="round" strokeLinejoin="round" d="M7.5 15h2.25m8.024-9.75c.011.05.028.1.052.148.591 1.2.924 2.55.924 3.977a8.96 8.96 0 0 1-.999 4.125m.023-8.25c-.076-.365.183-.75.575-.75h.908c.889 0 1.713.518 1.972 1.368.339 1.11.521 2.287.521 3.507 0 1.553-.295 3.036-.831 4.398C20.613 14.547 19.833 15 19.1 15h-.876c-.94 0-1.666.486-2.14 1.09a9.05 9.05 0 0 1-2.86 2.4c-.723.384-1.35.956-1.653 1.715a4.5 4.5 0 0 0-.322 1.672v.633a.75.75 0 0 1-.75.75 2.25 2.25 0 0 1-2.25-2.25c0-1.152.26-2.243.723-3.218.266-.558-.107-1.282-.725-1.282H5.622c-1.026 0-1.945-.694-2.054-1.715A12.04 12.04 0 0 1 3.5 13.5c0-2.848.992-5.464 2.649-7.521.388-.482.987-.729 1.605-.729H9.48c.483 0 .964.078 1.423.23l3.114 1.04c.459.153.94.23 1.423.23h.984" />
    </svg>
  );
}

function IconSpinner({ className }: { className?: string }) {
  return (
    <svg className={`playground-spinner ${className ?? ""}`} fill="none" viewBox="0 0 24 24">
      <circle opacity="0.2" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="3" />
      <path stroke="currentColor" strokeWidth="3" strokeLinecap="round" d="M12 2a10 10 0 0 1 10 10" />
    </svg>
  );
}

function IconExternalLink({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={1.5}>
      <path strokeLinecap="round" strokeLinejoin="round" d="M13.5 6H5.25A2.25 2.25 0 0 0 3 8.25v10.5A2.25 2.25 0 0 0 5.25 21h10.5A2.25 2.25 0 0 0 18 18.75V10.5m-10.5 6L21 3m0 0h-5.25M21 3v5.25" />
    </svg>
  );
}

/* ── Language tab icons ──────────────────────────── */

function LangIcon({ lang, className }: { lang: CodeLang; className?: string }) {
  if (lang === "python") {
    return (
      <svg className={className} viewBox="0 0 24 24" fill="currentColor">
        <path d="M11.914 0C5.82 0 6.2 2.656 6.2 2.656l.007 2.752h5.814v.826H3.9S0 5.789 0 11.969c0 6.18 3.403 5.96 3.403 5.96h2.03v-2.867s-.109-3.42 3.35-3.42h5.766s3.24.052 3.24-3.148V3.202S18.28 0 11.914 0ZM8.708 1.85a1.06 1.06 0 1 1 0 2.12 1.06 1.06 0 0 1 0-2.12Z" />
        <path d="M12.086 24c6.094 0 5.714-2.656 5.714-2.656l-.007-2.752h-5.814v-.826H20.1S24 18.211 24 12.031c0-6.18-3.403-5.96-3.403-5.96h-2.03v2.867s.109 3.42-3.35 3.42H9.451s-3.24-.052-3.24 3.148v5.292S5.72 24 12.086 24Zm3.206-1.85a1.06 1.06 0 1 1 0-2.12 1.06 1.06 0 0 1 0 2.12Z" />
      </svg>
    );
  }
  if (lang === "javascript") {
    return (
      <svg className={className} viewBox="0 0 24 24" fill="currentColor">
        <path d="M0 0h24v24H0V0Zm22.034 18.276c-.175-1.095-.888-2.015-3.003-2.873-.736-.345-1.554-.585-1.797-1.14-.091-.33-.105-.51-.046-.705.15-.646.915-.84 1.515-.66.39.12.75.42.976.9 1.034-.676 1.034-.676 1.755-1.125-.27-.42-.405-.6-.586-.78-.63-.705-1.469-1.065-2.834-1.034l-.705.089c-.676.165-1.32.525-1.71 1.005-1.14 1.291-.811 3.541.569 4.471 1.365 1.02 3.361 1.244 3.616 2.205.24 1.17-.87 1.545-1.966 1.41-.811-.18-1.26-.586-1.755-1.336l-1.83 1.051c.21.48.45.689.81 1.109 1.74 1.756 6.09 1.666 6.871-1.004.029-.09.24-.705.074-1.65l.046.067Zm-8.983-7.245h-2.248c0 1.938-.009 3.864-.009 5.805 0 1.232.063 2.363-.138 2.711-.33.689-1.18.601-1.566.48-.396-.196-.594-.466-.84-.855-.066-.119-.114-.21-.138-.21l-1.844 1.14c.309.63.756 1.17 1.324 1.517.855.494 2.004.675 3.207.405.783-.226 1.458-.691 1.811-1.411.51-.93.402-2.07.397-3.346.012-2.054 0-4.109 0-6.179l.004-.057Z" />
      </svg>
    );
  }
  if (lang === "go") {
    return (
      <svg className={className} viewBox="0 0 24 24" fill="currentColor">
        <path d="M1.811 10.231c-.047 0-.058-.023-.035-.059l.246-.315c.023-.035.081-.058.128-.058h4.172c.046 0 .058.035.035.07l-.199.303c-.023.036-.082.07-.117.07ZM.047 11.306c-.047 0-.059-.023-.035-.058l.245-.316c.023-.035.082-.058.129-.058h5.328c.047 0 .07.035.058.07l-.093.28c-.012.047-.058.07-.105.07ZM2.828 12.381c-.046 0-.058-.023-.035-.059l.163-.292c.023-.035.07-.07.117-.07h2.337c.047 0 .07.035.07.082l-.023.28c0 .047-.047.082-.082.082ZM18.615 10.01c-.726.187-1.222.327-1.948.514-.176.046-.187.058-.34-.117-.174-.198-.304-.327-.549-.444-.735-.362-1.445-.257-2.101.187-.773.526-1.171 1.3-1.16 2.218.012.921.642 1.678 1.551 1.806.793.105 1.457-.129 1.984-.736.105-.13.199-.268.316-.434H14.04c-.245 0-.304-.152-.222-.339.152-.362.432-.968.596-1.272a.315.315 0 0 1 .292-.187h4.253c-.023.317-.023.632-.07.948a5.555 5.555 0 0 1-1.04 2.473c-.897 1.18-2.042 1.97-3.502 2.218-1.201.199-2.332.024-3.34-.689-.935-.655-1.507-1.54-1.705-2.659-.234-1.318.082-2.53.783-3.64.784-1.237 1.878-2.077 3.27-2.484 1.143-.339 2.262-.293 3.316.339.71.422 1.226 1.027 1.577 1.77.07.13.023.187-.117.222Z" />
      </svg>
    );
  }
  // shell
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
      <path strokeLinecap="round" strokeLinejoin="round" d="m6.75 7.5 3 2.25-3 2.25m4.5 0h3" />
    </svg>
  );
}

/* ── Code panel (Tavily style) ───────────────────── */

function CodePanel({
  code,
  copyCode,
  lang,
  onChangeLang,
}: {
  code: string;
  copyCode: string;
  lang: CodeLang;
  onChangeLang: (l: CodeLang) => void;
}) {
  const [copied, setCopied] = useState(false);
  const tokens = useMemo(() => tokenizeCode(code, lang), [code, lang]);

  function handleCopy() {
    void navigator.clipboard.writeText(copyCode);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  const langs: { key: CodeLang; label: string }[] = [
    { key: "python", label: "Python" },
    { key: "javascript", label: "JavaScript" },
    { key: "shell", label: "Shell" },
    { key: "go", label: "Go" },
  ];

  return (
    <div className="flex h-full flex-col">
      {/* Language tabs */}
      <div className="flex items-center gap-1 border-b border-[var(--border)] px-6">
        {langs.map((l) => (
          <button
            key={l.key}
            type="button"
            onClick={() => onChangeLang(l.key)}
            className={`flex items-center gap-2 rounded-t-lg border-b-2 px-4 py-2.5 text-sm font-medium transition ${
              lang === l.key
                ? "border-[var(--foreground)] text-[var(--foreground)]"
                : "border-transparent text-[var(--foreground-tertiary)] hover:text-[var(--foreground-secondary)]"
            }`}
          >
            <LangIcon lang={l.key} className="h-4 w-4" />
            {l.label}
          </button>
        ))}
      </div>

      {/* Code block */}
      <div className="relative mx-6 mt-3 flex-1 overflow-hidden rounded-[12px] bg-[#1a1a2e]">
        {/* Copy button */}
        <button
          type="button"
          onClick={handleCopy}
          className="absolute right-3 top-3 z-10 flex items-center gap-1.5 rounded-md bg-white/10 px-3 py-1.5 text-xs font-medium text-white/60 backdrop-blur transition hover:bg-white/15 hover:text-white/90"
        >
          {copied ? <IconCheck className="h-3.5 w-3.5" /> : <IconCopy className="h-3.5 w-3.5" />}
          {copied ? "Copied!" : "Copy"}
        </button>

        <pre className="h-full overflow-auto p-5 pr-24 text-[15px] leading-[1.9]">
          <code>
            {tokens.map((t, i) => (
              t.color
                ? <span key={i} style={{ color: t.color }}>{t.text}</span>
                : <span key={i} className="text-[#e0e0e0]">{t.text}</span>
            ))}
          </code>
        </pre>
      </div>

      <div className="h-3" />
    </div>
  );
}

/* ── JSON panel (with line numbers + syntax color) ── */

function JsonPanel({ data }: { data: unknown }) {
  const [copied, setCopied] = useState(false);
  const formatted = useMemo(() => JSON.stringify(data, null, 2), [data]);
  const tokens = useMemo(() => tokenizeJson(formatted), [formatted]);
  const lineCount = useMemo(() => formatted.split("\n").length, [formatted]);

  function handleCopy() {
    void navigator.clipboard.writeText(formatted);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  return (
    <div className="relative flex h-full flex-col overflow-hidden rounded-[12px] bg-[#1a1a2e]">
      <button
        type="button"
        onClick={handleCopy}
        className="absolute right-3 top-3 z-10 flex items-center gap-1.5 rounded-md bg-white/10 px-3 py-1.5 text-xs font-medium text-white/60 backdrop-blur transition hover:bg-white/15 hover:text-white/90"
      >
        {copied ? <IconCheck className="h-3.5 w-3.5" /> : <IconCopy className="h-3.5 w-3.5" />}
        {copied ? "Copied!" : "Copy"}
      </button>

      <div className="flex flex-1 overflow-auto">
        {/* Line numbers */}
        <div className="sticky left-0 shrink-0 select-none border-r border-white/8 bg-[#1a1a2e] py-5 pl-4 pr-3 text-right text-[15px] leading-[1.9] text-white/20">
          {Array.from({ length: lineCount }, (_, i) => (
            <div key={i}>{i + 1}</div>
          ))}
        </div>
        {/* Code */}
        <pre className="flex-1 py-5 pl-4 pr-16 text-[15px] leading-[1.9]">
          <code>
            {tokens.map((t, i) => (
              t.color
                ? <span key={i} style={{ color: t.color }}>{t.text}</span>
                : <span key={i} className="text-[#e0e0e0]">{t.text}</span>
            ))}
          </code>
        </pre>
      </div>
    </div>
  );
}

/* ── Result preview card ─────────────────────────── */

function ResultCard({
  result,
  requestId,
}: {
  result: PlaygroundSearchResult;
  requestId: string;
}) {
  const [rating, setRating] = useState<1 | -1 | null>(null);
  const [feedbackPending, setFeedbackPending] = useState(false);
  const [imgLoaded, setImgLoaded] = useState(false);

  async function handleRate(value: 1 | -1) {
    if (feedbackPending) return;
    const nextRating = rating === value ? null : value;
    setFeedbackPending(true);
    try {
      await playground.feedback(requestId, result.id, nextRating);
      setRating(nextRating);
    } catch {
      // silent
    } finally {
      setFeedbackPending(false);
    }
  }

  const imageUrl = result.keyframeUrl || result.thumbnailUrl;

  return (
    <a
      href={result.url}
      target="_blank"
      rel="noreferrer"
      className="group block overflow-hidden rounded-[16px] border border-[var(--border)] bg-[var(--background-elevated,#ffffff)] transition hover:border-[var(--border-strong)] hover:shadow-sm"
    >
      {imageUrl ? (
        <div className="relative aspect-video w-full overflow-hidden bg-[rgba(36,29,21,0.04)]">
          {!imgLoaded ? (
            <div className="absolute inset-0 animate-pulse bg-[rgba(36,29,21,0.06)]" />
          ) : null}
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img
            src={imageUrl}
            alt={result.title}
            className={`h-full w-full object-cover transition-opacity duration-300 ${imgLoaded ? "opacity-100" : "opacity-0"}`}
            loading="lazy"
            decoding="async"
            onLoad={() => setImgLoaded(true)}
          />
          {result.duration > 0 ? (
            <span className="absolute bottom-2 right-2 rounded-md bg-black/70 px-2 py-0.5 text-[11px] font-medium text-white">
              {formatDuration(result.duration)}
            </span>
          ) : null}
          <div className="absolute inset-0 bg-black/0 transition group-hover:bg-black/5" />
        </div>
      ) : null}
      <div className="p-4">
        <div className="flex items-start justify-between gap-2">
          <h3 className="line-clamp-2 flex-1 text-sm font-semibold text-[var(--foreground)] group-hover:text-[var(--brand-bright)]">
            {result.title}
          </h3>
          <IconExternalLink className="mt-0.5 h-4 w-4 shrink-0 text-[var(--foreground-tertiary)] opacity-0 transition group-hover:opacity-100" />
        </div>
        {result.snippet ? (
          <p className="mt-1.5 line-clamp-3 text-xs leading-5 text-[var(--foreground-secondary)]">
            {result.snippet}
          </p>
        ) : null}
        <div className="mt-3 flex items-center justify-between">
          <div className="flex items-center gap-2 text-[11px] text-[var(--foreground-tertiary)]">
            {result.source ? <span>{result.source}</span> : null}
            {result.speaker ? (
              <>
                <span>&middot;</span>
                <span>{result.speaker}</span>
              </>
            ) : null}
            {result.score > 0 ? (
              <>
                <span>&middot;</span>
                <span>score {result.score.toFixed(3)}</span>
              </>
            ) : null}
          </div>
          <div className="flex items-center gap-1" onClick={(e) => e.preventDefault()}>
            <button
              type="button"
              onClick={(e) => { e.preventDefault(); void handleRate(1); }}
              disabled={feedbackPending}
              className={`rounded-md p-1.5 transition ${rating === 1 ? "text-[var(--brand-bright)]" : "text-[var(--foreground-tertiary)] hover:text-[var(--foreground)]"}`}
            >
              <IconThumbUp className="h-4 w-4" filled={rating === 1} />
            </button>
            <button
              type="button"
              onClick={(e) => { e.preventDefault(); void handleRate(-1); }}
              disabled={feedbackPending}
              className={`rounded-md p-1.5 transition ${rating === -1 ? "text-[var(--error,#bf5b46)]" : "text-[var(--foreground-tertiary)] hover:text-[var(--foreground)]"}`}
            >
              <IconThumbDown className="h-4 w-4" filled={rating === -1} />
            </button>
          </div>
        </div>
      </div>
    </a>
  );
}

/* ── Main component ──────────────────────────────── */

export function PlaygroundScreen() {
  const [query, setQuery] = useState("");
  const [keys, setKeys] = useState<DashboardApiKey[]>([]);
  const [selectedKeyId, setSelectedKeyId] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [keysLoading, setKeysLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [response, setResponse] = useState<PlaygroundSearchResponse | null>(null);
  const [rightPanel, setRightPanel] = useState<RightPanel>("code");
  const [codeLang, setCodeLang] = useState<CodeLang>("python");
  const [responseTab, setResponseTab] = useState<ResponseTab>("preview");
  const [includeAnswer, setIncludeAnswer] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [maxResults, setMaxResults] = useState(5);
  const [rankingMode, setRankingMode] = useState<"embedding" | "rerank">("embedding");
  const [includeSummary, setIncludeSummary] = useState(false);
  const [filterSpeaker, setFilterSpeaker] = useState("");
  const [filterSource, setFilterSource] = useState("");
  const [filterMinDuration, setFilterMinDuration] = useState("");
  const [filterMaxDuration, setFilterMaxDuration] = useState("");

  useEffect(() => {
    void (async () => {
      try {
        const items = await apiKeys.list();
        const active = items.filter((k) => k.isActive);
        setKeys(active);
        if (active.length > 0) {
          const defaultKey = active.find((k) => k.name.toLowerCase() === "default");
          setSelectedKeyId(defaultKey?.id ?? active[0].id);
        }
      } catch {
        // Silent
      } finally {
        setKeysLoading(false);
      }
    })();
  }, []);

  const selectedKey = keys.find((k) => k.id === selectedKeyId) ?? keys[0] ?? null;

  const codeSnippet = useMemo(
    () => buildCodeSnippet(codeLang, query, maskKey(selectedKey?.prefix ?? "cerul_xxxx")),
    [codeLang, query, selectedKey?.prefix],
  );

  const codeSnippetForCopy = useMemo(
    () => buildCodeSnippet(codeLang, query, selectedKey?.rawKey ?? maskKey(selectedKey?.prefix ?? "cerul_xxxx")),
    [codeLang, query, selectedKey?.rawKey, selectedKey?.prefix],
  );

  async function handleSearch() {
    if (!query.trim() || isLoading) return;
    if (!selectedKeyId) {
      setError("Select an API key before sending a playground request.");
      return;
    }
    setIsLoading(true);
    setError(null);
    try {
      const result = await playground.search(query.trim(), {
        apiKeyId: selectedKeyId,
        maxResults,
        includeAnswer,
        includeSummary,
        rankingMode,
        filters: {
          speaker: filterSpeaker || null,
          source: filterSource || null,
          minDuration: filterMinDuration ? Number(filterMinDuration) : null,
          maxDuration: filterMaxDuration ? Number(filterMaxDuration) : null,
        },
      });
      setResponse(result);
      setRightPanel("response");
      setResponseTab("preview");
    } catch (e) {
      setError(getApiErrorMessage(e, "Search request failed."));
    } finally {
      setIsLoading(false);
    }
  }

  const rawJson = useMemo(() => {
    if (!response) return null;
    return {
      query,
      request_id: response.requestId,
      credits_used: response.creditsUsed,
      credits_remaining: response.creditsRemaining,
      answer: response.answer,
      results: response.results.map((r) => ({
        id: r.id, score: r.score, rerank_score: r.rerankScore,
        url: r.url, title: r.title, snippet: r.snippet,
        transcript: r.transcript, thumbnail_url: r.thumbnailUrl,
        keyframe_url: r.keyframeUrl, duration: r.duration,
        source: r.source, speaker: r.speaker,
        published_at: r.publishedAt, language: r.language,
        timestamp_start: r.timestampStart, timestamp_end: r.timestampEnd,
      })),
    };
  }, [response, query]);

  return (
    <DashboardLayout
      currentPath="/dashboard/playground"
      title=""
      actions={null}
    >
      <div className="grid h-[calc(100vh-130px)] gap-6 lg:grid-cols-[minmax(0,0.85fr)_minmax(0,1.4fr)]">
        {/* ── Left panel ─────────────────────────────── */}
        <div className="flex flex-col gap-5 overflow-y-auto">
          <div className="surface-elevated dashboard-card flex flex-col gap-5 rounded-[24px] p-6 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden" style={{ overflowX: "hidden", overflowY: "auto" }}>
            {/* Query */}
            <div>
              <label className="mb-1.5 flex items-center gap-2 text-sm font-semibold text-[var(--foreground)]">
                Query
                <span className="rounded-md bg-[rgba(191,91,70,0.1)] px-1.5 py-0.5 text-[10px] font-semibold text-[rgba(191,91,70,0.8)]">
                  required
                </span>
              </label>
              <textarea
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Describe what you're looking for..."
                rows={4}
                className="w-full resize-y rounded-[12px] border border-[var(--border)] bg-white px-4 py-3 text-sm text-[var(--foreground)] outline-none transition placeholder:text-[var(--foreground-tertiary)] focus:border-[var(--border-brand)]"
                onKeyDown={(e) => {
                  if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                    void handleSearch();
                  }
                }}
              />
            </div>

            {/* Include answer */}
            <div>
              <label className="mb-1.5 flex items-center gap-1.5 text-sm font-semibold text-[var(--foreground)]">
                Include answer
                <span className="group relative cursor-help">
                  <svg className="h-4 w-4 text-[var(--foreground-tertiary)]" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={1.5}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="m11.25 11.25.041-.02a.75.75 0 0 1 1.063.852l-.708 2.836a.75.75 0 0 0 1.063.853l.041-.021M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Zm-9-3.75h.008v.008H12V8.25Z" />
                  </svg>
                  <span className="pointer-events-none absolute left-0 top-full z-50 mt-2 w-56 rounded-lg bg-[var(--foreground)] px-3 py-2 text-xs leading-relaxed text-white opacity-0 shadow-lg transition group-hover:opacity-100">
                    Include an LLM-generated answer to the provided query. When enabled, returns a synthesized response based on the search results.
                  </span>
                </span>
              </label>
              <select
                value={includeAnswer ? "true" : "false"}
                onChange={(e) => setIncludeAnswer(e.target.value === "true")}
                className="w-full rounded-[12px] border border-[var(--border)] bg-white px-4 py-2.5 text-sm text-[var(--foreground)] outline-none transition focus:border-[var(--border-brand)]"
              >
                <option value="false">No</option>
                <option value="true">Yes</option>
              </select>
            </div>

            {/* Try example */}
            <button
              type="button"
              onClick={() => setQuery("Find the segment where the speaker explains how transformer attention works")}
              className="flex items-center gap-1.5 self-start rounded-full border border-[var(--border)] px-3.5 py-1.5 text-xs font-medium text-[var(--foreground-tertiary)] transition hover:border-[var(--border-strong)] hover:text-[var(--foreground-secondary)]"
            >
              <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M9.813 15.904 9 18.75l-.813-2.846a4.5 4.5 0 0 0-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 0 0 3.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 0 0 3.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 0 0-3.09 3.09ZM18.259 8.715 18 9.75l-.259-1.035a3.375 3.375 0 0 0-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 0 0 2.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 0 0 2.455 2.456L21.75 6l-1.036.259a3.375 3.375 0 0 0-2.455 2.456Z" />
              </svg>
              Try an example
            </button>

            {/* Send button */}
            <button
              type="button"
              disabled={!isLoading && (!query.trim() || keys.length === 0 || !selectedKeyId)}
              onClick={() => void handleSearch()}
              className={`button-primary w-full ${isLoading ? "!opacity-100 pointer-events-none" : ""}`}
            >
              {isLoading ? (
                <span className="flex items-center justify-center gap-2">
                  <IconSpinner className="h-5 w-5" />
                  Sending...
                </span>
              ) : (
                "Send"
              )}
            </button>

            {/* Additional fields */}
            <button
              type="button"
              onClick={() => setShowAdvanced(!showAdvanced)}
              className="flex items-center gap-2 text-sm font-semibold text-[var(--foreground)]"
            >
              <svg className={`h-4 w-4 text-[var(--foreground-tertiary)] transition ${showAdvanced ? "rotate-180" : ""}`} fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="m19.5 8.25-7.5 7.5-7.5-7.5" />
              </svg>
              Additional fields
              <svg className="h-4 w-4 text-[var(--foreground-tertiary)]" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={1.5}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M11.42 15.17 17.25 21A2.652 2.652 0 0 0 21 17.25l-5.877-5.877M11.42 15.17l2.496-3.03c.317-.384.74-.626 1.208-.766M11.42 15.17l-4.655 5.653a2.548 2.548 0 1 1-3.586-3.586l6.837-5.63m5.108-.233c.55-.164 1.163-.188 1.743-.14a4.5 4.5 0 0 0 4.486-6.336l-3.276 3.277a3.004 3.004 0 0 1-2.25-2.25l3.276-3.276a4.5 4.5 0 0 0-6.336 4.486c.091 1.076-.071 2.264-.904 2.95l-.102.085m-1.745 1.437L5.909 7.5H4.5L2.25 3.75l1.5-1.5L7.5 4.5v1.409l4.26 4.26m-1.745 1.437 1.745-1.437m6.615 8.206L15.75 15.75M4.867 19.125h.008v.008h-.008v-.008Z" />
              </svg>
            </button>

            {showAdvanced ? (
              <div className="space-y-4 border-t border-[var(--border)] pt-4 pb-16">
                {/* API Key */}
                <div>
                  <label className="mb-1.5 flex items-center gap-1.5 text-sm font-semibold text-[var(--foreground)]">
                    <svg className="h-4 w-4 text-[var(--foreground-tertiary)]" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={1.5}>
                      <path strokeLinecap="round" strokeLinejoin="round" d="M15.75 5.25a3 3 0 0 1 3 3m3 0a6 6 0 0 1-7.029 5.912c-.563-.097-1.159.026-1.563.43L10.5 17.25H8.25v2.25H6v2.25H2.25v-2.818c0-.597.237-1.17.659-1.591l6.499-6.499c.404-.404.527-1 .43-1.563A6 6 0 1 1 21.75 8.25Z" />
                    </svg>
                    API Key
                  </label>
                  {keysLoading ? (
                    <div className="h-10 animate-pulse rounded-[12px] bg-[rgba(36,29,21,0.06)]" />
                  ) : (
                    <select
                      value={selectedKeyId ?? ""}
                      onChange={(e) => setSelectedKeyId(e.target.value)}
                      className="w-full rounded-[12px] border border-[var(--border)] bg-white px-4 py-2.5 text-sm text-[var(--foreground)] outline-none transition focus:border-[var(--border-brand)]"
                    >
                      {keys.map((k) => (
                        <option key={k.id} value={k.id}>{k.name}</option>
                      ))}
                    </select>
                  )}
                </div>

                {/* Max results + Ranking mode */}
                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className="mb-1.5 flex items-center gap-1.5 text-sm font-semibold text-[var(--foreground)]">
                      Max results
                      <span className="group relative cursor-help">
                        <svg className="h-4 w-4 text-[var(--foreground-tertiary)]" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={1.5}>
                          <path strokeLinecap="round" strokeLinejoin="round" d="m11.25 11.25.041-.02a.75.75 0 0 1 1.063.852l-.708 2.836a.75.75 0 0 0 1.063.853l.041-.021M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Zm-9-3.75h.008v.008H12V8.25Z" />
                        </svg>
                        <span className="pointer-events-none absolute left-0 top-full z-50 mt-2 w-56 rounded-lg bg-[var(--foreground)] px-3 py-2 text-xs leading-relaxed text-white opacity-0 shadow-lg transition group-hover:opacity-100">
                          Number of results to return (1-20).
                        </span>
                      </span>
                    </label>
                    <input
                      type="number"
                      min={1}
                      max={20}
                      value={maxResults}
                      onChange={(e) => setMaxResults(Math.min(20, Math.max(1, Number(e.target.value) || 5)))}
                      className="w-full rounded-[12px] border border-[var(--border)] bg-white px-4 py-2.5 text-sm text-[var(--foreground)] outline-none transition focus:border-[var(--border-brand)]"
                    />
                  </div>
                  <div>
                    <label className="mb-1.5 flex items-center gap-1.5 text-sm font-semibold text-[var(--foreground)]">
                      Ranking mode
                      <span className="group relative cursor-help">
                        <svg className="h-4 w-4 text-[var(--foreground-tertiary)]" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={1.5}>
                          <path strokeLinecap="round" strokeLinejoin="round" d="m11.25 11.25.041-.02a.75.75 0 0 1 1.063.852l-.708 2.836a.75.75 0 0 0 1.063.853l.041-.021M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Zm-9-3.75h.008v.008H12V8.25Z" />
                        </svg>
                        <span className="pointer-events-none absolute left-0 top-full z-50 mt-2 w-56 rounded-lg bg-[var(--foreground)] px-3 py-2 text-xs leading-relaxed text-white opacity-0 shadow-lg transition group-hover:opacity-100">
                          &quot;embedding&quot; for fast vector search, &quot;rerank&quot; for higher-quality LLM re-ranking.
                        </span>
                      </span>
                    </label>
                    <select
                      value={rankingMode}
                      onChange={(e) => setRankingMode(e.target.value as "embedding" | "rerank")}
                      className="w-full rounded-[12px] border border-[var(--border)] bg-white px-4 py-2.5 text-sm text-[var(--foreground)] outline-none transition focus:border-[var(--border-brand)]"
                    >
                      <option value="embedding">Embedding</option>
                      <option value="rerank">Rerank</option>
                    </select>
                  </div>
                </div>

                {/* Include summary */}
                <div>
                  <label className="mb-1.5 flex items-center gap-1.5 text-sm font-semibold text-[var(--foreground)]">
                    Include summary
                    <span className="group relative cursor-help">
                      <svg className="h-4 w-4 text-[var(--foreground-tertiary)]" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={1.5}>
                        <path strokeLinecap="round" strokeLinejoin="round" d="m11.25 11.25.041-.02a.75.75 0 0 1 1.063.852l-.708 2.836a.75.75 0 0 0 1.063.853l.041-.021M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Zm-9-3.75h.008v.008H12V8.25Z" />
                      </svg>
                      <span className="pointer-events-none absolute left-0 top-full z-50 mt-2 w-56 rounded-lg bg-[var(--foreground)] px-3 py-2 text-xs leading-relaxed text-white opacity-0 shadow-lg transition group-hover:opacity-100">
                        Add a brief summary of each result snippet.
                      </span>
                    </span>
                  </label>
                  <select
                    value={includeSummary ? "true" : "false"}
                    onChange={(e) => setIncludeSummary(e.target.value === "true")}
                    className="w-full rounded-[12px] border border-[var(--border)] bg-white px-4 py-2.5 text-sm text-[var(--foreground)] outline-none transition focus:border-[var(--border-brand)]"
                  >
                    <option value="false">No</option>
                    <option value="true">Yes</option>
                  </select>
                </div>

                {/* Filters */}
                <div>
                  <p className="mb-2 flex items-center gap-1.5 text-sm font-semibold text-[var(--foreground)]">
                    <svg className="h-4 w-4 text-[var(--foreground-tertiary)]" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={1.5}>
                      <path strokeLinecap="round" strokeLinejoin="round" d="M12 3c2.755 0 5.455.232 8.083.678.533.09.917.556.917 1.096v1.044a2.25 2.25 0 0 1-.659 1.591l-5.432 5.432a2.25 2.25 0 0 0-.659 1.591v2.927a2.25 2.25 0 0 1-1.244 2.013L9.75 21v-6.568a2.25 2.25 0 0 0-.659-1.591L3.659 7.409A2.25 2.25 0 0 1 3 5.818V4.774c0-.54.384-1.006.917-1.096A48.32 48.32 0 0 1 12 3Z" />
                    </svg>
                    Filters
                  </p>
                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className="mb-1 block text-xs text-[var(--foreground-secondary)]">Speaker</label>
                      <input
                        type="text"
                        value={filterSpeaker}
                        onChange={(e) => setFilterSpeaker(e.target.value)}
                        placeholder="e.g. Andrej Karpathy"
                        className="w-full rounded-[10px] border border-[var(--border)] bg-white px-3 py-2 text-sm text-[var(--foreground)] outline-none transition placeholder:text-[var(--foreground-tertiary)] focus:border-[var(--border-brand)]"
                      />
                    </div>
                    <div>
                      <label className="mb-1 block text-xs text-[var(--foreground-secondary)]">Source</label>
                      <input
                        type="text"
                        value={filterSource}
                        onChange={(e) => setFilterSource(e.target.value)}
                        placeholder="e.g. youtube"
                        className="w-full rounded-[10px] border border-[var(--border)] bg-white px-3 py-2 text-sm text-[var(--foreground)] outline-none transition placeholder:text-[var(--foreground-tertiary)] focus:border-[var(--border-brand)]"
                      />
                    </div>
                    <div>
                      <label className="mb-1 block text-xs text-[var(--foreground-secondary)]">Min duration (s)</label>
                      <input
                        type="number"
                        min={0}
                        value={filterMinDuration}
                        onChange={(e) => setFilterMinDuration(e.target.value)}
                        placeholder="0"
                        className="w-full rounded-[10px] border border-[var(--border)] bg-white px-3 py-2 text-sm text-[var(--foreground)] outline-none transition placeholder:text-[var(--foreground-tertiary)] focus:border-[var(--border-brand)]"
                      />
                    </div>
                    <div>
                      <label className="mb-1 block text-xs text-[var(--foreground-secondary)]">Max duration (s)</label>
                      <input
                        type="number"
                        min={0}
                        value={filterMaxDuration}
                        onChange={(e) => setFilterMaxDuration(e.target.value)}
                        placeholder="Any"
                        className="w-full rounded-[10px] border border-[var(--border)] bg-white px-3 py-2 text-sm text-[var(--foreground)] outline-none transition placeholder:text-[var(--foreground-tertiary)] focus:border-[var(--border-brand)]"
                      />
                    </div>
                  </div>
                </div>
              </div>
            ) : null}
          </div>

          {error ? (
            <div className="rounded-[16px] border border-[rgba(191,91,70,0.2)] bg-[rgba(191,91,70,0.08)] px-4 py-3 text-sm text-[var(--error,#bf5b46)]">
              {error}
            </div>
          ) : null}
        </div>

        {/* ── Right panel ────────────────────────────── */}
        <div className="flex flex-col overflow-hidden rounded-[24px] border border-[var(--border)] bg-[var(--background-elevated,white)]">
          {/* Header row: title + Response/Code pill */}
          <div className="flex items-center justify-between px-6 pt-4 pb-2">
            <h2 className="text-xl font-bold text-[var(--foreground)]">
              {rightPanel === "code" ? "Code" : response ? "Response" : "Code"}
            </h2>
            <div className="flex items-center gap-1 rounded-full border border-[var(--border)] bg-white p-1">
              <button
                type="button"
                onClick={() => setRightPanel("response")}
                className={`flex items-center gap-1.5 rounded-full px-4 py-1.5 text-sm font-medium transition ${
                  rightPanel === "response"
                    ? "bg-[var(--foreground)] text-white shadow-sm"
                    : "text-[var(--foreground-tertiary)] hover:text-[var(--foreground-secondary)]"
                }`}
              >
                <IconResponse className="h-4 w-4" />
                Response
              </button>
              <button
                type="button"
                onClick={() => setRightPanel("code")}
                className={`flex items-center gap-1.5 rounded-full px-4 py-1.5 text-sm font-medium transition ${
                  rightPanel === "code"
                    ? "bg-[var(--foreground)] text-white shadow-sm"
                    : "text-[var(--foreground-tertiary)] hover:text-[var(--foreground-secondary)]"
                }`}
              >
                <IconCode className="h-4 w-4" />
                Code
              </button>
            </div>
          </div>

          {/* Sub-header: Preview/JSON tabs or language tabs */}
          {rightPanel === "response" && response ? (
            <div className="flex items-center gap-1 px-6 pb-2">
              <div className="flex gap-1 rounded-lg border border-[var(--border)] bg-white p-1">
                <button
                  type="button"
                  onClick={() => setResponseTab("preview")}
                  className={`flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium transition ${
                    responseTab === "preview"
                      ? "bg-[var(--foreground)] text-white shadow-sm"
                      : "text-[var(--foreground-tertiary)] hover:text-[var(--foreground-secondary)]"
                  }`}
                >
                  <svg className="h-3.5 w-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={1.5}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="m2.25 15.75 5.159-5.159a2.25 2.25 0 0 1 3.182 0l5.159 5.159m-1.5-1.5 1.409-1.409a2.25 2.25 0 0 1 3.182 0l2.909 2.909M3.75 21h16.5A2.25 2.25 0 0 0 22.5 18.75V5.25A2.25 2.25 0 0 0 20.25 3H3.75A2.25 2.25 0 0 0 1.5 5.25v13.5A2.25 2.25 0 0 0 3.75 21Z" />
                  </svg>
                  Preview
                </button>
                <button
                  type="button"
                  onClick={() => setResponseTab("json")}
                  className={`flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium transition ${
                    responseTab === "json"
                      ? "bg-[var(--foreground)] text-white shadow-sm"
                      : "text-[var(--foreground-tertiary)] hover:text-[var(--foreground-secondary)]"
                  }`}
                >
                  {"{ } JSON"}
                </button>
              </div>
            </div>
          ) : null}

          {/* Panel body */}
          <div className="flex-1 overflow-auto">
            {rightPanel === "code" ? (
              <CodePanel code={codeSnippet} copyCode={codeSnippetForCopy} lang={codeLang} onChangeLang={setCodeLang} />
            ) : !response ? (
              <div className="flex h-full items-center justify-center text-sm text-[var(--foreground-tertiary)]">
                Send a request to see the response here.
              </div>
            ) : (
              <div className="flex h-full flex-col">
                {/* Tab content */}
                <div className="flex-1 overflow-auto px-6 pb-6">
                  {responseTab === "json" && rawJson ? (
                    <JsonPanel data={rawJson} />
                  ) : responseTab === "preview" ? (
                    <div className="space-y-4">
                      {response.answer ? (
                        <div className="rounded-[12px] border border-[var(--border)] bg-[rgba(111, 151, 201,0.06)] p-4 text-sm leading-6 text-[var(--foreground)]">
                          <p className="mb-1.5 text-xs font-semibold uppercase tracking-wide text-[var(--foreground-tertiary)]">
                            Answer
                          </p>
                          {response.answer}
                        </div>
                      ) : null}

                      {response.results.length === 0 ? (
                        <div className="py-10 text-center text-sm text-[var(--foreground-tertiary)]">
                          No results found.
                        </div>
                      ) : (
                        <div className="grid gap-4">
                          {response.results.map((result) => (
                            <ResultCard key={result.id} result={result} requestId={response.requestId} />
                          ))}
                        </div>
                      )}

                      <div className="flex items-center justify-between text-xs text-[var(--foreground-tertiary)]">
                        <span>
                          {response.results.length} result{response.results.length !== 1 ? "s" : ""}
                          {" "}&middot; {response.creditsUsed} credit{response.creditsUsed !== 1 ? "s" : ""} used
                        </span>
                        <span className="font-mono">{response.requestId}</span>
                      </div>
                    </div>
                  ) : null}
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </DashboardLayout>
  );
}
