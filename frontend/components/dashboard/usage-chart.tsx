import type { UsageChartPoint } from "@/lib/dashboard";
import { formatNumber } from "@/lib/dashboard";

type UsageChartProps = {
  title: string;
  description: string;
  data: UsageChartPoint[];
};

function niceMaxValue(max: number): number {
  if (max <= 0) return 10;
  const magnitude = Math.pow(10, Math.floor(Math.log10(max)));
  const normalized = max / magnitude;
  if (normalized <= 1) return magnitude;
  if (normalized <= 2) return 2 * magnitude;
  if (normalized <= 5) return 5 * magnitude;
  return 10 * magnitude;
}

export function UsageChart({ title, description, data }: UsageChartProps) {
  const recentData = data;

  if (recentData.length === 0) {
    return (
      <article className="surface-elevated dashboard-card rounded-[24px] px-6 py-6">
        <div className="flex items-start gap-3">
          <span className="mt-0.5 inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-[12px] bg-[rgba(111, 151, 201,0.12)]">
            <svg className="h-[18px] w-[18px] text-[var(--brand-bright)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path d="M3 13.125C3 12.504 3.504 12 4.125 12h2.25c.621 0 1.125.504 1.125 1.125v6.75C7.5 20.496 6.996 21 6.375 21h-2.25A1.125 1.125 0 0 1 3 19.875v-6.75ZM9.75 8.625c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125v11.25c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 0 1-1.125-1.125V8.625ZM16.5 4.125c0-.621.504-1.125 1.125-1.125h2.25C20.496 3 21 3.504 21 4.125v15.75c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 0 1-1.125-1.125V4.125Z" strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} />
            </svg>
          </span>
          <div>
            <h2 className="text-lg font-semibold text-[var(--foreground)]">{title}</h2>
            <p className="mt-1 text-sm text-[var(--foreground-secondary)]">{description}</p>
          </div>
        </div>
        <div className="mt-6 rounded-[18px] border border-dashed border-[var(--border)] px-5 py-10 text-center text-sm text-[var(--foreground-secondary)]">
          No usage has been recorded for this period yet.
        </div>
      </article>
    );
  }

  const width = 920;
  const height = 240;
  const paddingLeft = 52;
  const paddingRight = 16;
  const paddingTop = 20;
  const paddingBottom = 32;
  const usableWidth = width - paddingLeft - paddingRight;
  const usableHeight = height - paddingTop - paddingBottom;

  const creditValues = recentData.map((p) => p.creditsUsed);
  const rawMax = Math.max(0, ...creditValues);
  const maxValue = niceMaxValue(rawMax);
  const gridLines = 4;
  const totalCredits = creditValues.reduce((sum, v) => sum + v, 0);

  // Build smooth line path
  const points = creditValues.map((value, index) => {
    const x = recentData.length === 1
      ? paddingLeft + usableWidth / 2
      : paddingLeft + (index / (recentData.length - 1)) * usableWidth;
    const y = paddingTop + usableHeight - (value / maxValue) * usableHeight;
    return { x, y };
  });

  const linePath = points.map((p, i) => `${i === 0 ? "M" : "L"} ${p.x} ${p.y}`).join(" ");
  const areaPath = `${linePath} L ${points[points.length - 1].x} ${paddingTop + usableHeight} L ${points[0].x} ${paddingTop + usableHeight} Z`;

  // X-axis labels: show ~7 evenly spaced dates
  const labelInterval = Math.max(1, Math.floor(recentData.length / 7));

  return (
    <article className="surface-elevated dashboard-card rounded-[24px] px-6 py-5">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2.5">
          <span className="inline-flex h-8 w-8 items-center justify-center rounded-[10px] bg-[rgba(111, 151, 201,0.12)]">
            <svg className="h-4 w-4 text-[var(--brand-bright)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path d="M3 13.125C3 12.504 3.504 12 4.125 12h2.25c.621 0 1.125.504 1.125 1.125v6.75C7.5 20.496 6.996 21 6.375 21h-2.25A1.125 1.125 0 0 1 3 19.875v-6.75ZM9.75 8.625c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125v11.25c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 0 1-1.125-1.125V8.625ZM16.5 4.125c0-.621.504-1.125 1.125-1.125h2.25C20.496 3 21 3.504 21 4.125v15.75c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 0 1-1.125-1.125V4.125Z" strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} />
            </svg>
          </span>
          <h2 className="text-base font-semibold text-[var(--foreground)]">{title}</h2>
        </div>
        <div className="flex items-center gap-2 text-sm text-[var(--foreground-secondary)]">
          <span className="h-2 w-5 rounded-full bg-[var(--brand)]" />
          {formatNumber(totalCredits)} credits · {recentData.length} days
        </div>
      </div>

      <div className="mt-4 overflow-hidden rounded-[18px] border border-[var(--border)] bg-[var(--background-elevated)] p-3">
        <svg viewBox={`0 0 ${width} ${height}`} className="w-full">
          <defs>
            <linearGradient id="credit-fill" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="rgba(111, 151, 201, 0.28)" />
              <stop offset="100%" stopColor="rgba(111, 151, 201, 0.02)" />
            </linearGradient>
          </defs>

          {/* Y-axis grid lines + labels */}
          {Array.from({ length: gridLines + 1 }).map((_, i) => {
            const y = paddingTop + (usableHeight / gridLines) * i;
            const value = Math.round(maxValue - (maxValue / gridLines) * i);
            return (
              <g key={i}>
                <line
                  x1={paddingLeft}
                  x2={width - paddingRight}
                  y1={y}
                  y2={y}
                  stroke="rgba(40, 54, 74, 0.08)"
                />
                <text
                  x={paddingLeft - 8}
                  y={y + 4}
                  fill="rgba(109,101,88,0.56)"
                  fontSize="11"
                  textAnchor="end"
                  fontFamily="var(--font-mono, monospace)"
                >
                  {formatNumber(value)}
                </text>
              </g>
            );
          })}

          {/* Area + Line */}
          <path d={areaPath} fill="url(#credit-fill)" />
          <path d={linePath} fill="none" stroke="var(--brand)" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />

          {/* Data points */}
          {points.map((p, i) => (
            <circle key={recentData[i].date} cx={p.x} cy={p.y} r="3" fill="var(--brand-bright)" />
          ))}

          {/* X-axis date labels */}
          {recentData.map((point, index) => {
            if (index % labelInterval !== 0 && index !== recentData.length - 1) return null;
            const x = recentData.length === 1
              ? paddingLeft + usableWidth / 2
              : paddingLeft + (index / (recentData.length - 1)) * usableWidth;
            return (
              <text
                key={point.date}
                x={x}
                y={height - 6}
                fill="rgba(109,101,88,0.56)"
                fontSize="11"
                textAnchor="middle"
              >
                {point.shortLabel}
              </text>
            );
          })}
        </svg>
      </div>
    </article>
  );
}
