import type { AdminMetricValue } from "@/lib/admin-api";
import {
  formatAdminDelta,
  formatAdminMetricValue,
  formatTargetStatus,
  getMetricTone,
} from "@/lib/admin-console";

type AdminMetricCardProps = {
  label: string;
  metric: AdminMetricValue;
  note?: string;
  kind?: "number" | "percent" | "milliseconds";
};

export function AdminMetricCard({
  label,
  metric,
  note,
  kind = "number",
}: AdminMetricCardProps) {
  const tone = getMetricTone(metric);
  const toneMeta =
    tone === "good"
      ? {
          card: "border-[rgba(31,141,74,0.18)] bg-[linear-gradient(180deg,rgba(214,241,224,0.96),rgba(255,252,247,0.92))]",
          badge: "border-[rgba(31,141,74,0.16)] bg-[rgba(31,141,74,0.1)] text-[var(--success)]",
          accent: "from-[rgba(31,141,74,0.22)] via-[rgba(31,141,74,0.04)] to-transparent",
          state: "On target",
        }
      : tone === "warning"
        ? {
            card: "border-[rgba(177,132,24,0.18)] bg-[linear-gradient(180deg,rgba(250,239,214,0.98),rgba(255,252,247,0.92))]",
            badge: "border-[rgba(177,132,24,0.16)] bg-[rgba(177,132,24,0.1)] text-[var(--warning)]",
            accent: "from-[rgba(212,156,105,0.28)] via-[rgba(212,156,105,0.06)] to-transparent",
            state: "Needs review",
          }
        : {
            card: "border-[var(--border)] bg-[linear-gradient(180deg,rgba(255,255,255,0.96),rgba(252,246,237,0.94))]",
            badge: "border-[var(--border)] bg-white/74 text-[var(--foreground-secondary)]",
            accent: "from-[rgba(111, 151, 201,0.22)] via-[rgba(111, 151, 201,0.04)] to-transparent",
            state: "Benchmark",
          };

  return (
    <article
      className={`surface-elevated rounded-[30px] px-5 py-5 ${toneMeta.card}`}
    >
      <div
        className={`pointer-events-none absolute inset-x-0 top-0 h-20 bg-gradient-to-b ${toneMeta.accent}`}
      />
      <div className="relative">
        <div className="flex items-start justify-between gap-3">
          <p className="font-mono text-[11px] uppercase tracking-[0.18em] text-[var(--foreground-tertiary)]">
            {label}
          </p>
          <span
            className={`inline-flex items-center rounded-full border px-3 py-1 font-mono text-[11px] uppercase tracking-[0.16em] ${toneMeta.badge}`}
          >
            {toneMeta.state}
          </span>
        </div>
        <p className="mt-5 text-4xl font-semibold tracking-[-0.05em] text-[var(--foreground)]">
          {formatAdminMetricValue(metric.current, { kind, compact: true })}
        </p>
        {note ? (
          <p className="mt-3 text-sm leading-7 text-[var(--foreground-secondary)]">
            {note}
          </p>
        ) : null}
        <div className="mt-5 flex flex-wrap gap-2 text-xs">
          <span className="rounded-full border border-[var(--border)] bg-white/68 px-3 py-1.5 text-[var(--foreground-secondary)]">
            Delta {formatAdminDelta(metric, { kind })}
          </span>
          {metric.target !== null ? (
            <span className="rounded-full border border-[var(--border)] bg-white/68 px-3 py-1.5 text-[var(--foreground-secondary)]">
              {formatTargetStatus(metric, { kind })}
            </span>
          ) : null}
        </div>
      </div>
    </article>
  );
}
