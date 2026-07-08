"use client";

import Link from "next/link";
import { Fragment, useEffect, useMemo, useState } from "react";
import {
  getApiErrorMessage,
  jobs,
  type DashboardJobDetail,
  type DashboardJobSummary,
  type DashboardJobStats,
  type JobStatus,
} from "@/lib/api";
import { formatDashboardDateTime, formatNumber } from "@/lib/dashboard";
import { AdminLayout } from "@/components/admin/admin-layout";
import {
  DashboardNotice,
  DashboardSkeleton,
  DashboardState,
} from "./dashboard-state";
import { useJobList, useJobStats } from "./use-jobs";
import { SceneRouteSummary } from "@/components/admin/scene-route-summary";

type JobSort = "created_desc" | "created_asc" | "duration_desc" | "duration_asc" | "status";
type StatusFilter = JobStatus | "all";

const PAGE_SIZE_OPTIONS = [10, 25, 50] as const;

const STATUS_OPTIONS: Array<{ label: string; value: StatusFilter }> = [
  { label: "All", value: "all" },
  { label: "Pending", value: "pending" },
  { label: "Running", value: "running" },
  { label: "Retrying", value: "retrying" },
  { label: "Completed", value: "completed" },
  { label: "Failed", value: "failed" },
];

const SORT_OPTIONS: Array<{ label: string; value: JobSort }> = [
  { label: "Newest first", value: "created_desc" },
  { label: "Oldest first", value: "created_asc" },
  { label: "Longest duration", value: "duration_desc" },
  { label: "Shortest duration", value: "duration_asc" },
  { label: "Status", value: "status" },
];

const STATUS_ORDER: Record<JobStatus, number> = {
  running: 0,
  retrying: 1,
  pending: 2,
  failed: 3,
  completed: 4,
};

function toDate(value: string | null | undefined): Date | null {
  if (!value) {
    return null;
  }

  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date;
}

function getDurationMs(job: Pick<DashboardJobSummary, "startedAt" | "completedAt" | "updatedAt">): number | null {
  const start = toDate(job.startedAt);

  if (!start) {
    return null;
  }

  const end = toDate(job.completedAt) ?? toDate(job.updatedAt);

  if (!end) {
    return null;
  }

  return Math.max(0, end.getTime() - start.getTime());
}

function formatDuration(job: Pick<DashboardJobSummary, "startedAt" | "completedAt" | "updatedAt" | "status">): string {
  const durationMs = getDurationMs(job);

  if (durationMs === null) {
    return "Not started";
  }

  const totalSeconds = Math.max(1, Math.round(durationMs / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const parts: string[] = [];

  if (hours > 0) {
    parts.push(`${hours}h`);
  }

  if (minutes > 0 || hours > 0) {
    parts.push(`${minutes}m`);
  }

  parts.push(`${seconds}s`);

  const formatted = parts.join(" ");
  return job.completedAt || job.status === "completed" || job.status === "failed"
    ? formatted
    : `${formatted} so far`;
}

function formatDateTime(value: string | null | undefined, fallback: string): string {
  return value ? formatDashboardDateTime(value) : fallback;
}

function getStatusLabel(status: JobStatus): string {
  return status.charAt(0).toUpperCase() + status.slice(1);
}

function getStatusBadgeClass(status: JobStatus): string {
  if (status === "completed") {
    return "badge-success";
  }

  if (status === "failed") {
    return "badge-error";
  }

  return "badge-warning";
}

function getStatusDotClass(status: JobStatus): string {
  if (status === "completed") {
    return "bg-emerald-400 shadow-[0_0_16px_rgba(74,222,128,0.45)]";
  }

  if (status === "failed") {
    return "bg-rose-400 shadow-[0_0_16px_rgba(248,113,113,0.45)]";
  }

  return "bg-amber-300 shadow-[0_0_16px_rgba(253,224,71,0.4)]";
}

function getStepToken(
  stepStatus: "pending" | "running" | "completed" | "failed" | "skipped",
): string {
  if (stepStatus === "running") {
    return "RN";
  }

  if (stepStatus === "pending") {
    return "PD";
  }

  if (stepStatus === "completed") {
    return "OK";
  }

  if (stepStatus === "failed") {
    return "ER";
  }

  return "SK";
}

function getStepTokenClasses(
  stepStatus: "pending" | "running" | "completed" | "failed" | "skipped",
): string {
  if (stepStatus === "running") {
    return "border-blue-500/30 bg-blue-500/15 text-blue-200";
  }

  if (stepStatus === "pending") {
    return "border-slate-700/40 bg-slate-700/20 text-slate-300";
  }

  if (stepStatus === "completed") {
    return "border-emerald-500/30 bg-emerald-500/15 text-emerald-200";
  }

  if (stepStatus === "failed") {
    return "border-rose-500/30 bg-rose-500/15 text-rose-200";
  }

  return "border-slate-500/30 bg-slate-500/15 text-slate-200";
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function summarizeArtifacts(artifacts: unknown): string {
  if (Array.isArray(artifacts)) {
    return artifacts.length === 0
      ? "No artifacts recorded."
      : `${artifacts.length} artifact item(s) recorded.`;
  }

  if (isPlainObject(artifacts)) {
    const entries = Object.entries(artifacts).filter(([key]) => {
      return ![
        "logs",
        "guidance",
        "duration_ms",
        "timeout_seconds",
        "current_route",
        "analysis_route",
        "route_counts",
        "annotation_frame_count",
        "total_annotation_frame_count",
        "extraction_time_ms",
        "dedup_time_ms",
        "filter_time_ms",
        "ocr_time_ms",
        "prepare_time_ms",
        "annotation_time_ms",
        "total_extraction_time_ms",
        "total_dedup_time_ms",
        "total_filter_time_ms",
        "total_ocr_time_ms",
        "total_prepare_time_ms",
        "total_annotation_time_ms",
      ].includes(key);
    });

    if (entries.length === 0) {
      return "No artifacts recorded.";
    }

    return entries
      .slice(0, 3)
      .map(([key, value]) => `${key}: ${String(value)}`)
      .join(" | ");
  }

  if (artifacts === null || artifacts === undefined) {
    return "No artifacts recorded.";
  }

  return String(artifacts);
}

function formatStepDuration(
  step: Pick<DashboardJobDetail["steps"][number], "durationMs" | "startedAt" | "completedAt" | "updatedAt" | "status">,
): string {
  if (typeof step.durationMs === "number" && Number.isFinite(step.durationMs)) {
    const totalSeconds = Math.max(1, Math.round(step.durationMs / 1000));
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return minutes > 0 ? `${minutes}m ${seconds}s` : `${seconds}s`;
  }

  const start = toDate(step.startedAt);
  const end = toDate(step.completedAt) ?? toDate(step.updatedAt);
  if (!start || !end) {
    return "Not recorded";
  }
  const totalSeconds = Math.max(1, Math.round((end.getTime() - start.getTime()) / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return minutes > 0 ? `${minutes}m ${seconds}s` : `${seconds}s`;
}

function getPayloadSummary(payload: unknown): string {
  if (Array.isArray(payload)) {
    return `${payload.length} payload item(s)`;
  }

  if (isPlainObject(payload)) {
    const keys = Object.keys(payload);
    return keys.length === 0 ? "Empty payload" : `${keys.length} payload field(s)`;
  }

  return "Opaque payload";
}

function getPayloadSource(payload: unknown): string | null {
  if (!isPlainObject(payload)) {
    return null;
  }

  const source = payload.source;
  return typeof source === "string" && source.trim() ? source : null;
}

function sortJobs(items: DashboardJobSummary[], sort: JobSort): DashboardJobSummary[] {
  const jobsBySort = [...items];

  jobsBySort.sort((jobA, jobB) => {
    if (sort === "created_asc") {
      return jobA.createdAt.localeCompare(jobB.createdAt);
    }

    if (sort === "duration_desc") {
      return (getDurationMs(jobB) ?? -1) - (getDurationMs(jobA) ?? -1);
    }

    if (sort === "duration_asc") {
      return (getDurationMs(jobA) ?? Number.MAX_SAFE_INTEGER)
        - (getDurationMs(jobB) ?? Number.MAX_SAFE_INTEGER);
    }

    if (sort === "status") {
      const statusDelta = STATUS_ORDER[jobA.status] - STATUS_ORDER[jobB.status];
      return statusDelta !== 0
        ? statusDelta
        : jobB.createdAt.localeCompare(jobA.createdAt);
    }

    return jobB.createdAt.localeCompare(jobA.createdAt);
  });

  return jobsBySort;
}

function StatOverview({
  stats,
}: {
  stats: DashboardJobStats;
}) {
  const activeQueue = stats.pending + stats.running + stats.retrying;
  const completionRate =
    stats.total === 0 ? 0 : Math.round((stats.completed / stats.total) * 100);
  const cards = [
    {
      label: "Active queue",
      value: formatNumber(activeQueue),
      note: "Pending, running, and retrying jobs still consuming admin attention.",
      tone: "text-[var(--brand-bright)]",
      shell:
        "border-[var(--border-brand)] bg-[rgba(143, 176, 216,0.08)] shadow-[0_18px_40px_rgba(62, 107, 157,0.12)]",
    },
    {
      label: "Completion rate",
      value: `${completionRate}%`,
      note: "Share of observed jobs that have landed in a completed state.",
      tone: "text-white",
      shell: "border-[var(--border)] bg-[var(--surface)]",
    },
    {
      label: "Terminal failures",
      value: formatNumber(stats.failed),
      note: "Jobs that exhausted retries or hit a hard terminal stop.",
      tone: "text-rose-100",
      shell: "border-rose-500/25 bg-rose-500/10",
    },
  ] as const;

  return (
    <section className="grid gap-5 xl:grid-cols-[1.12fr_0.88fr]">
      <article className="surface-elevated rounded-[32px] px-6 py-6">
        <div className="flex flex-col gap-5 lg:flex-row lg:items-start lg:justify-between">
          <div className="max-w-2xl">
            <p className="eyebrow">Pipeline telemetry</p>
            <h2 className="mt-4 text-4xl font-semibold tracking-[-0.05em] text-white sm:text-5xl">
              Watch the queue, retries, and final outcomes in one surface.
            </h2>
            <p className="mt-4 text-base leading-8 text-[var(--foreground-secondary)]">
              This panel is the admin-facing stream for worker health. Use it to spot retry
              pressure early, compare pipeline mix, and decide which jobs need a deeper forensic read.
            </p>
          </div>
          <span className="label label-brand">Live ledger</span>
        </div>

        <div className="mt-8 grid gap-4 lg:grid-cols-3">
          {cards.map((card) => (
            <article
              key={card.label}
              className={`rounded-[22px] border px-5 py-5 ${card.shell}`}
            >
              <p className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                {card.label}
              </p>
              <p className={`mt-3 text-4xl font-semibold tracking-[-0.04em] ${card.tone}`}>
                {card.value}
              </p>
              <p className="mt-2 text-sm leading-6 text-[var(--foreground-secondary)]">
                {card.note}
              </p>
            </article>
          ))}
        </div>

        <div className="mt-8 grid gap-5 lg:grid-cols-[1.05fr_0.95fr]">
          <div className="rounded-[24px] border border-[var(--border)] bg-[rgba(255,255,255,0.03)] px-5 py-5">
            <div className="flex items-center justify-between gap-3">
              <div>
                <p className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                  Queue snapshot
                </p>
                <h3 className="mt-2 text-2xl font-semibold text-white">
                  Pending, running, and completed work
                </h3>
              </div>
              <span className="badge badge-success">Stable</span>
            </div>
            <div className="mt-5 space-y-4">
              {[
                {
                  label: "Pending",
                  value: stats.pending,
                  tone: "label-accent",
                  fill: "from-[var(--accent)] to-[var(--accent-bright)]",
                },
                {
                  label: "Running",
                  value: stats.running + stats.retrying,
                  tone: "label-brand",
                  fill: "from-[var(--brand)] to-[var(--brand-deep)]",
                },
                {
                  label: "Completed",
                  value: stats.completed,
                  tone: "label",
                  fill: "from-slate-400 to-slate-200",
                },
              ].map((item) => (
                <div key={item.label}>
                  <div className="flex items-center justify-between gap-3">
                    <span className={`label ${item.tone}`}>{item.label}</span>
                    <span className="font-mono text-sm text-white">
                      {formatNumber(item.value)} jobs
                    </span>
                  </div>
                  <div className="mt-3 h-3 rounded-full bg-[rgba(255,255,255,0.05)]">
                    <div
                      className={`h-full rounded-full bg-gradient-to-r ${item.fill}`}
                      style={{ width: `${stats.total > 0 ? Math.round((item.value / stats.total) * 100) : 0}%` }}
                    />
                  </div>
                  <p className="mt-2 text-sm leading-6 text-[var(--foreground-secondary)]">
                    {stats.total > 0 ? Math.round((item.value / stats.total) * 100) : 0}% of all observed pipeline jobs in the current telemetry set.
                  </p>
                </div>
              ))}
            </div>
          </div>

          <div className="grid gap-4">
            {[
              {
                label: "Pending",
                value: stats.pending,
                tone: "badge-warning",
                note: "Queued and waiting for a worker claim.",
              },
              {
                label: "Running",
                value: stats.running,
                tone: "badge-success",
                note: `Retrying queue currently holds ${formatNumber(stats.retrying)} job(s).`,
              },
              {
                label: "Completed",
                value: stats.completed,
                tone: "badge-success",
                note: "Finished cleanly without requiring admin intervention.",
              },
            ].map((item) => (
              <div
                key={item.label}
                className="rounded-[22px] border border-[var(--border)] bg-[var(--surface)] px-5 py-5"
              >
                <div className="flex items-center justify-between gap-3">
                  <p className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                    {item.label}
                  </p>
                  <span className={`badge ${item.tone}`}>{item.label}</span>
                </div>
                <p className="mt-3 text-3xl font-semibold tracking-[-0.04em] text-white">
                  {formatNumber(item.value)}
                </p>
                <p className="mt-2 text-sm leading-6 text-[var(--foreground-secondary)]">
                  {item.note}
                </p>
              </div>
            ))}
          </div>
        </div>
      </article>

      <div className="space-y-5">
        <article className="surface-gradient rounded-[32px] px-6 py-6">
          <p className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--brand-bright)]">
            Operator note
          </p>
          <h3 className="mt-3 text-3xl font-semibold tracking-[-0.04em] text-white">
            Retry pressure stays visible before jobs fully fail.
          </h3>
          <p className="mt-3 text-sm leading-7 text-[var(--foreground-secondary)]">
            Expanded rows below are the fastest path to step-level artifacts, payload summary, and
            final error text. Use the filters to narrow to the exact queue state you need.
          </p>
        </article>

        <article className="surface rounded-[28px] px-5 py-5">
          <p className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
            Total jobs
          </p>
          <p className="mt-3 text-4xl font-semibold tracking-[-0.04em] text-white">
            {formatNumber(stats.total)}
          </p>
          <p className="mt-2 text-sm leading-6 text-[var(--foreground-secondary)]">
            All jobs written into the processing ledger, across every pipeline and current state.
          </p>
        </article>

        <article className="surface rounded-[28px] px-5 py-5">
          <p className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
            Needs review
          </p>
          <div className="mt-4 flex items-center justify-between gap-3">
            <span className="badge badge-error">Terminal failures</span>
            <span className="font-mono text-2xl font-semibold text-white">
              {formatNumber(stats.failed)}
            </span>
          </div>
          <p className="mt-3 text-sm leading-6 text-[var(--foreground-secondary)]">
            Correlate these against expanded job traces and the worker-side logs before requeueing.
          </p>
        </article>
      </div>
    </section>
  );
}

function ExpandedJobDetail({
  detail,
}: {
  detail: DashboardJobDetail;
}) {
  return (
    <div className="grid gap-5 xl:grid-cols-[340px_minmax(0,1fr)]">
      <div className="space-y-4">
        <article className="rounded-[22px] border border-[var(--border)] bg-[var(--surface)] px-5 py-5">
          <div className="flex items-start justify-between gap-3">
            <div>
              <p className="font-mono text-xs uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                Job context
              </p>
              <h3 className="mt-2 text-xl font-semibold text-white">
                {detail.jobType}
              </h3>
            </div>
            <span className={`badge ${getStatusBadgeClass(detail.status)}`}>
              {getStatusLabel(detail.status)}
            </span>
          </div>

          <div className="mt-5 grid gap-3">
            {[
              {
                label: "Job ID",
                value: detail.id,
              },
              {
                label: "Source",
                value: getPayloadSource(detail.inputPayload) ?? "Unknown source",
              },
              {
                label: "Attempts",
                value: `${detail.attempts} / ${detail.maxAttempts}`,
              },
              {
                label: "Payload",
                value: getPayloadSummary(detail.inputPayload),
              },
              {
                label: "Started",
                value: formatDateTime(detail.startedAt, "Not started"),
              },
              {
                label: "Completed",
                value: formatDateTime(detail.completedAt, "Still in progress"),
              },
              {
                label: "Next retry",
                value: formatDateTime(detail.nextRetryAt, "No retry scheduled"),
              },
              {
                label: "Source ID",
                value: detail.sourceId ?? "Not attached",
              },
            ].map((item) => (
              <div
                key={item.label}
                className="rounded-[16px] border border-[var(--border)] bg-[var(--surface-elevated)] px-4 py-3"
              >
                <p className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                  {item.label}
                </p>
                <p className="mt-2 break-all font-mono text-sm text-white">
                  {item.value}
                </p>
              </div>
            ))}
          </div>
        </article>

        {detail.errorMessage ? (
          <DashboardNotice
            description={detail.errorMessage}
            title="Terminal job error"
            tone="error"
          />
        ) : null}
      </div>

      <article className="rounded-[22px] border border-[var(--border)] bg-[var(--surface)] px-5 py-5">
        <p className="font-mono text-xs uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
          Pipeline timeline
        </p>
        <h3 className="mt-2 text-xl font-semibold text-white">
          Step progression
        </h3>

        {detail.steps.length === 0 ? (
          <p className="mt-5 text-sm leading-6 text-[var(--foreground-secondary)]">
            No step records have been written for this job yet.
          </p>
        ) : (
          <div className="relative mt-5 space-y-4 before:absolute before:bottom-4 before:left-[1.18rem] before:top-4 before:w-px before:bg-[var(--border)]">
            {detail.steps.map((step) => (
              <div key={step.id} className="relative pl-12">
                <span
                  className={`absolute left-0 top-1 inline-flex h-9 w-9 items-center justify-center rounded-full border font-mono text-[11px] font-semibold ${getStepTokenClasses(step.status)}`}
                >
                  {getStepToken(step.status)}
                </span>
                <div className="rounded-[18px] border border-[var(--border)] bg-[var(--surface-elevated)] px-4 py-4">
                  <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
                    <div>
                      <p className="font-mono text-xs uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                        {step.stepName}
                      </p>
                      <p className="mt-2 text-sm leading-6 text-[var(--foreground-secondary)]">
                        {summarizeArtifacts(step.artifacts)}
                      </p>
                    </div>
                    <span
                      className={`badge ${
                        step.status === "completed"
                          ? "badge-success"
                          : step.status === "failed"
                            ? "badge-error"
                            : "badge-warning"
                      }`}
                    >
                      {step.status}
                    </span>
                  </div>

                  <div className="mt-4 grid gap-3 sm:grid-cols-2">
                    <div className="rounded-[14px] border border-[var(--border)] bg-[var(--surface)] px-3 py-3">
                      <p className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                        Started
                      </p>
                      <p className="mt-2 font-mono text-xs text-white">
                        {formatDateTime(step.startedAt, "Not recorded")}
                      </p>
                    </div>
                    <div className="rounded-[14px] border border-[var(--border)] bg-[var(--surface)] px-3 py-3">
                      <p className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                        Completed
                      </p>
                      <p className="mt-2 font-mono text-xs text-white">
                        {formatDateTime(step.completedAt, "Not completed")}
                      </p>
                    </div>
                    <div className="rounded-[14px] border border-[var(--border)] bg-[var(--surface)] px-3 py-3">
                      <p className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                        Duration
                      </p>
                      <p className="mt-2 font-mono text-xs text-white">
                        {formatStepDuration(step)}
                      </p>
                    </div>
                    <div className="rounded-[14px] border border-[var(--border)] bg-[var(--surface)] px-3 py-3">
                      <p className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                        Guidance
                      </p>
                      <p className="mt-2 text-xs leading-5 text-white">
                        {step.guidance ?? "No guidance recorded"}
                      </p>
                    </div>
                  </div>

                  <SceneRouteSummary artifacts={step.artifacts} status={step.status} variant="detail" />

                  {step.errorMessage ? (
                    <div className="mt-4 rounded-[14px] border border-rose-500/30 bg-rose-500/10 px-3 py-3 text-sm leading-6 text-rose-100">
                      {step.errorMessage}
                    </div>
                  ) : null}

                  {step.logs.length ? (
                    <div className="mt-4 rounded-[14px] border border-[var(--border)] bg-[var(--surface)] px-3 py-3">
                      <p className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                        Step logs
                      </p>
                      <div className="mt-3 space-y-2">
                        {step.logs.map((entry, index) => (
                          <div
                            key={`${entry.at ?? "log"}-${index}`}
                            className="rounded-[12px] bg-[var(--surface-elevated)] px-3 py-2"
                          >
                            <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-[var(--foreground-tertiary)]">
                              {(entry.level || "info").toUpperCase()} •{" "}
                              {formatDateTime(entry.at, "No timestamp")}
                            </p>
                            <p className="mt-1 text-sm leading-6 text-white">{entry.message}</p>
                            {entry.details ? (
                              <p className="mt-1 break-all font-mono text-[11px] text-[var(--foreground-secondary)]">
                                {summarizeArtifacts(entry.details)}
                              </p>
                            ) : null}
                          </div>
                        ))}
                      </div>
                    </div>
                  ) : null}
                </div>
              </div>
            ))}
          </div>
        )}
      </article>
    </div>
  );
}

export function DashboardPipelinesScreen() {
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [pageSize, setPageSize] = useState<(typeof PAGE_SIZE_OPTIONS)[number]>(25);
  const [offset, setOffset] = useState(0);
  const [sort, setSort] = useState<JobSort>("created_desc");
  const [expandedJobId, setExpandedJobId] = useState<string | null>(null);
  const [jobDetails, setJobDetails] = useState<Record<string, DashboardJobDetail>>({});
  const [jobDetailErrors, setJobDetailErrors] = useState<Record<string, string>>({});
  const [loadingJobIds, setLoadingJobIds] = useState<Record<string, boolean>>({});

  const listParams = useMemo(
    () => ({
      status: statusFilter === "all" ? undefined : statusFilter,
      limit: pageSize,
      offset,
    }),
    [offset, pageSize, statusFilter],
  );
  const {
    data: listData,
    error: listError,
    isLoading: isListLoading,
    refresh: refreshList,
  } = useJobList(listParams);
  const {
    data: statsData,
    error: statsError,
    isLoading: isStatsLoading,
    refresh: refreshStats,
  } = useJobStats();

  const sortedJobs = useMemo(
    () => sortJobs(listData?.jobs ?? [], sort),
    [listData?.jobs, sort],
  );
  const totalPages = listData ? Math.max(1, Math.ceil(listData.totalCount / pageSize)) : 1;
  const currentPage = Math.floor(offset / pageSize) + 1;
  const isInitialLoading =
    (isStatsLoading && !statsData) || (isListLoading && !listData);
  const hasFatalLoadError =
    !statsData &&
    !listData &&
    Boolean(statsError || listError) &&
    !isInitialLoading;
  const hasActiveFilters =
    statusFilter !== "all" ||
    pageSize !== 25 ||
    sort !== "created_desc";

  useEffect(() => {
    if (expandedJobId && !sortedJobs.some((job) => job.id === expandedJobId)) {
      setExpandedJobId(null);
    }
  }, [expandedJobId, sortedJobs]);

  async function refreshAll() {
    await Promise.all([refreshStats(), refreshList()]);
  }

  async function loadJobDetail(jobId: string) {
    setLoadingJobIds((current) => ({ ...current, [jobId]: true }));
    setJobDetailErrors((current) => {
      const next = { ...current };
      delete next[jobId];
      return next;
    });

    try {
      const detail = await jobs.get(jobId);
      setJobDetails((current) => ({ ...current, [jobId]: detail }));
    } catch (error) {
      setJobDetailErrors((current) => ({
        ...current,
        [jobId]: getApiErrorMessage(error, "Failed to load job detail."),
      }));
    } finally {
      setLoadingJobIds((current) => {
        const next = { ...current };
        delete next[jobId];
        return next;
      });
    }
  }

  async function handleToggleJob(jobId: string) {
    if (expandedJobId === jobId) {
      setExpandedJobId(null);
      return;
    }

    setExpandedJobId(jobId);

    if (!jobDetails[jobId] && !loadingJobIds[jobId]) {
      await loadJobDetail(jobId);
    }
  }

  function resetFilters() {
    setStatusFilter("all");
    setPageSize(25);
    setOffset(0);
    setSort("created_desc");
  }

  return (
    <AdminLayout
      actions={
        <>
          <Link className="button-secondary" href="/docs">
            Architecture guide
          </Link>
          <button className="button-primary" onClick={() => void refreshAll()} type="button">
            Refresh
          </button>
        </>
      }
      currentPath="/admin/pipelines"
      description="Inspect job-by-job worker telemetry, retry posture, and step artifacts from the shared processing pipeline."
      title="Pipelines"
    >
      {isInitialLoading ? (
        <DashboardSkeleton />
      ) : hasFatalLoadError ? (
        <DashboardState
          action={
            <button className="button-primary" onClick={() => void refreshAll()} type="button">
              Retry request
            </button>
          }
          description={statsError ?? listError ?? "Pipeline telemetry could not be loaded."}
          title="Pipeline telemetry could not be loaded"
          tone="error"
        />
      ) : (
        <>
          {statsError && statsData ? (
            <DashboardNotice
              description={statsError}
              title="Job stats could not be refreshed. The overview below shows the last successful snapshot."
              tone="error"
            />
          ) : null}

          {listError && listData ? (
            <DashboardNotice
              description={listError}
              title="The recent jobs table could not be refreshed. Showing the last successful response."
              tone="error"
            />
          ) : null}

          {statsData ? (
            <StatOverview stats={statsData} />
          ) : (
            <DashboardState
              action={
                <button className="button-primary" onClick={() => void refreshStats()} type="button">
                  Retry stats
                </button>
              }
              description={statsError ?? "Pipeline stats are unavailable right now."}
              title="Stats overview unavailable"
              tone="error"
            />
          )}

          {listData ? (
            listData.totalCount === 0 && statusFilter === "all" ? (
              <DashboardState
                action={
                  <Link className="button-secondary" href="/docs">
                    Read pipeline docs
                  </Link>
                }
                description="No processing jobs have been written yet. Once workers start processing or indexing sources, recent jobs and step timelines will appear here."
                title="No pipeline jobs yet"
              />
            ) : (
              <section className="surface-elevated overflow-hidden rounded-[32px]">
                <div className="border-b border-[var(--border)] px-6 py-6">
                  <div className="flex flex-col gap-5 xl:flex-row xl:items-end xl:justify-between">
                    <div>
                      <p className="eyebrow">Job ledger</p>
                      <h2 className="mt-3 text-3xl font-semibold tracking-[-0.04em] text-white">
                        Recent processing jobs
                      </h2>
                      <p className="mt-3 max-w-3xl text-sm leading-7 text-[var(--foreground-secondary)]">
                        Showing {formatNumber(sortedJobs.length)} visible job(s) from{" "}
                        {formatNumber(listData.totalCount)} total entries that match the current
                        queue view.
                      </p>
                    </div>

                    <div className="flex flex-wrap items-center gap-2">
                      <span className="label label-brand">
                        {statusFilter === "all"
                          ? "All statuses"
                          : getStatusLabel(statusFilter)}
                      </span>
                      <span className="label">
                        {formatNumber(listData.totalCount)} total
                      </span>
                      {hasActiveFilters ? (
                        <button
                          className="button-secondary"
                          onClick={resetFilters}
                          type="button"
                        >
                          Reset filters
                        </button>
                      ) : null}
                    </div>
                  </div>

                  <div className="mt-6 grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
                    <label className="rounded-[20px] border border-[var(--border)] bg-[var(--surface)] px-4 py-3">
                      <span className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                        Status
                      </span>
                      <select
                        className="mt-2 w-full rounded-[14px] border border-[var(--border)] bg-[var(--surface-elevated)] px-3 py-2 text-sm text-white outline-none transition focus:border-[var(--border-brand)]"
                        onChange={(event) => {
                          setStatusFilter(event.target.value as StatusFilter);
                          setOffset(0);
                        }}
                        value={statusFilter}
                      >
                        {STATUS_OPTIONS.map((option) => (
                          <option key={option.value} value={option.value}>
                            {option.label}
                          </option>
                        ))}
                      </select>
                    </label>

                    <label className="rounded-[20px] border border-[var(--border)] bg-[var(--surface)] px-4 py-3">
                      <span className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                        Sort by
                      </span>
                      <select
                        className="mt-2 w-full rounded-[14px] border border-[var(--border)] bg-[var(--surface-elevated)] px-3 py-2 text-sm text-white outline-none transition focus:border-[var(--border-brand)]"
                        onChange={(event) => {
                          setSort(event.target.value as JobSort);
                        }}
                        value={sort}
                      >
                        {SORT_OPTIONS.map((option) => (
                          <option key={option.value} value={option.value}>
                            {option.label}
                          </option>
                        ))}
                      </select>
                    </label>

                    <label className="rounded-[20px] border border-[var(--border)] bg-[var(--surface)] px-4 py-3">
                      <span className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                        Page size
                      </span>
                      <select
                        className="mt-2 w-full rounded-[14px] border border-[var(--border)] bg-[var(--surface-elevated)] px-3 py-2 text-sm text-white outline-none transition focus:border-[var(--border-brand)]"
                        onChange={(event) => {
                          setPageSize(
                            Number(event.target.value) as (typeof PAGE_SIZE_OPTIONS)[number],
                          );
                          setOffset(0);
                        }}
                        value={pageSize}
                      >
                        {PAGE_SIZE_OPTIONS.map((option) => (
                          <option key={option} value={option}>
                            {option}
                          </option>
                        ))}
                      </select>
                    </label>
                  </div>
                </div>

                {sortedJobs.length === 0 ? (
                  <div className="px-6 py-8">
                    <DashboardState
                      action={
                        <button className="button-secondary" onClick={resetFilters} type="button">
                          Clear filters
                        </button>
                      }
                      description="No jobs match the current status filter. Clear the controls to return to the full recent job stream."
                      title="No matching jobs"
                    />
                  </div>
                ) : (
                  <>
                    <div className="px-4 py-4 sm:px-6 sm:py-6">
                      <div className="space-y-4">
                        {sortedJobs.map((job) => {
                          const isExpanded = expandedJobId === job.id;
                          const detail = jobDetails[job.id];
                          const detailError = jobDetailErrors[job.id];
                          const isLoadingDetail = Boolean(loadingJobIds[job.id]);
                          const jobMetrics = [
                            {
                              label: "Attempts",
                              value: `${job.attempts} / ${job.maxAttempts}`,
                            },
                            {
                              label: "Created",
                              value: formatDashboardDateTime(job.createdAt),
                            },
                            {
                              label: "Updated",
                              value: formatDashboardDateTime(job.updatedAt),
                            },
                            {
                              label: "Duration",
                              value: formatDuration(job),
                            },
                          ] as const;

                          return (
                            <Fragment key={job.id}>
                              <button
                                aria-expanded={isExpanded}
                                className={`w-full rounded-[28px] border text-left transition ${
                                  isExpanded
                                    ? "border-[var(--border-brand)] bg-[rgba(143, 176, 216,0.08)] shadow-[0_18px_44px_rgba(62, 107, 157,0.14)]"
                                    : "border-[var(--border)] bg-[rgba(255,255,255,0.02)] hover:border-[var(--border-strong)] hover:bg-white/[0.04]"
                                }`}
                                onClick={() => void handleToggleJob(job.id)}
                                type="button"
                              >
                                <div className="flex flex-col gap-5 px-5 py-5 xl:flex-row xl:items-start xl:justify-between">
                                  <div className="min-w-0 flex-1">
                                    <div className="flex flex-wrap items-center gap-2">
                                      <span className={`h-2.5 w-2.5 rounded-full ${getStatusDotClass(job.status)}`} />
                                      <span className={`badge ${getStatusBadgeClass(job.status)}`}>
                                        {getStatusLabel(job.status)}
                                      </span>
                                      {job.errorMessage ? (
                                        <span className="badge badge-error">Attention</span>
                                      ) : null}
                                      <span className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                                        {isExpanded ? "Collapse trace" : "Inspect trace"}
                                      </span>
                                    </div>

                                    <div className="mt-4 flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
                                      <div className="min-w-0">
                                        <h3 className="text-2xl font-semibold text-white">
                                          {job.jobType}
                                        </h3>
                                        <p className="mt-2 break-all font-mono text-xs uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                                          {job.id}
                                        </p>
                                        <p
                                          className={`mt-3 max-w-2xl text-sm leading-6 ${
                                            job.errorMessage
                                              ? "text-rose-100"
                                              : "text-[var(--foreground-secondary)]"
                                          }`}
                                        >
                                          {job.errorMessage ??
                                            "No terminal error recorded. Expand this row for payload context, step-by-step artifacts, and retry timing."}
                                        </p>
                                      </div>

                                      <span className="inline-flex min-h-[2.25rem] items-center rounded-full border border-[var(--border)] px-4 text-sm text-white">
                                        {isExpanded ? "Hide details" : "Open details"}
                                      </span>
                                    </div>
                                  </div>

                                  <div className="grid gap-3 sm:grid-cols-2 xl:min-w-[460px] xl:grid-cols-4">
                                    {jobMetrics.map((metric) => (
                                      <div
                                        key={metric.label}
                                        className="rounded-[18px] border border-[var(--border)] bg-[rgba(8,11,18,0.42)] px-4 py-4"
                                      >
                                        <p className="font-mono text-[11px] uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                                          {metric.label}
                                        </p>
                                        <p className="mt-2 text-sm font-semibold leading-6 text-white">
                                          {metric.value}
                                        </p>
                                      </div>
                                    ))}
                                  </div>
                                </div>
                              </button>

                              {isExpanded ? (
                                <div className="rounded-[28px] border border-[var(--border)] bg-[rgba(255,255,255,0.02)] p-3 sm:p-4">
                                  {isLoadingDetail && !detail ? (
                                    <div className="rounded-[22px] border border-[var(--border)] bg-[var(--surface)] px-5 py-5">
                                      <div className="animate-pulse space-y-3">
                                        <div className="h-4 w-32 rounded-full bg-white/10" />
                                        <div className="h-8 w-56 rounded-full bg-white/10" />
                                        <div className="h-3 w-full rounded-full bg-white/10" />
                                        <div className="h-3 w-5/6 rounded-full bg-white/10" />
                                      </div>
                                    </div>
                                  ) : detailError && !detail ? (
                                    <div className="rounded-[22px] border border-[var(--border)] bg-[var(--surface)] px-5 py-5">
                                      <DashboardNotice
                                        description={detailError}
                                        title="Job detail could not be loaded"
                                        tone="error"
                                      />
                                      <div className="mt-4">
                                        <button
                                          className="button-primary"
                                          onClick={() => void loadJobDetail(job.id)}
                                          type="button"
                                        >
                                          Retry detail
                                        </button>
                                      </div>
                                    </div>
                                  ) : detail ? (
                                    <ExpandedJobDetail detail={detail} />
                                  ) : null}
                                </div>
                              ) : null}
                            </Fragment>
                          );
                        })}
                      </div>
                    </div>

                    <div className="flex flex-col gap-4 border-t border-[var(--border)] px-6 py-5 sm:flex-row sm:items-center sm:justify-between">
                      <div>
                        <p className="font-mono text-xs uppercase tracking-[0.16em] text-[var(--foreground-tertiary)]">
                          Page {formatNumber(currentPage)} of {formatNumber(totalPages)}
                        </p>
                        <p className="mt-2 text-sm leading-6 text-[var(--foreground-secondary)]">
                          Each page preserves the current filters and sort order while you step
                          through the queue.
                        </p>
                      </div>
                      <div className="flex flex-wrap gap-3">
                        <button
                          className="button-secondary disabled:cursor-not-allowed disabled:opacity-60"
                          disabled={offset === 0}
                          onClick={() => {
                            setOffset((current) => Math.max(0, current - pageSize));
                          }}
                          type="button"
                        >
                          Previous
                        </button>
                        <button
                          className="button-secondary disabled:cursor-not-allowed disabled:opacity-60"
                          disabled={offset + pageSize >= listData.totalCount}
                          onClick={() => {
                            setOffset((current) => current + pageSize);
                          }}
                          type="button"
                        >
                          Next
                        </button>
                      </div>
                    </div>
                  </>
                )}
              </section>
            )
          ) : (
            <DashboardState
              action={
                <button className="button-primary" onClick={() => void refreshList()} type="button">
                  Retry jobs
                </button>
              }
              description={listError ?? "Recent processing jobs are unavailable right now."}
              title="Job ledger unavailable"
              tone="error"
            />
          )}
        </>
      )}
    </AdminLayout>
  );
}
