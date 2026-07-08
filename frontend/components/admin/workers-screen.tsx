"use client";

import { useCallback, useEffect, useState } from "react";
import {
  admin,
  type AdminFailedJob,
  type AdminRange,
  type AdminWorkerNode,
} from "@/lib/admin-api";
import { getApiErrorMessage } from "@/lib/api";
import { formatAdminDateTime } from "@/lib/admin-console";
import { AdminLayout } from "./admin-layout";
import { AdminRangePicker } from "./admin-range-picker";
import { WorkerLivePanel } from "./worker-live-panel";
import {
  DashboardNotice,
  DashboardSkeleton,
  DashboardState,
} from "@/components/dashboard/dashboard-state";
import { useAdminResource } from "./use-admin-resource";

const INTEGER_FORMATTER = new Intl.NumberFormat("en-US");
const WORKER_NODE_POLL_INTERVAL_MS = 15_000;

function formatCount(value: number) {
  return INTEGER_FORMATTER.format(value);
}

function formatDuration(valueMs: number): string {
  const totalSeconds = Math.max(Math.floor(valueMs / 1000), 0);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }

  if (minutes > 0) {
    return `${minutes}m ${seconds}s`;
  }

  return `${seconds}s`;
}

function formatRelativeTime(value: string, referenceNowMs: number): string {
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp)) {
    return "—";
  }

  return `${formatDuration(Math.max(referenceNowMs - timestamp, 0))} ago`;
}

function formatSourceLabel(job: AdminFailedJob): string {
  if (job.sourceName) return job.sourceName;
  if (job.sourceSlug) return job.sourceSlug;
  if (job.sourceId) return job.sourceId.slice(0, 8);
  return "—";
}

function buildVideoUrl(job: AdminFailedJob): string | null {
  if (job.videoUrl) {
    return job.videoUrl;
  }
  if (job.videoId) {
    return `https://youtube.com/watch?v=${job.videoId}`;
  }
  return null;
}

type StatCardProps = {
  label: string;
  value: number;
  accentColor: string;
  glowColor: string;
  children?: React.ReactNode;
};

function StatCard({ label, value, accentColor, glowColor, children }: StatCardProps) {
  return (
    <div className="surface-elevated relative flex h-full flex-col justify-between rounded-[28px] p-5">
      <div className={`absolute bottom-0 left-0 top-0 w-1 ${accentColor}`} />
      <span className="text-xs font-semibold uppercase tracking-wider text-[var(--foreground-tertiary)]">
        {label}
      </span>
      <div className="mb-3 mt-2 text-3xl font-bold text-[var(--foreground)]">{formatCount(value)}</div>
      {children}
      <div className={`pointer-events-none absolute -bottom-4 -right-4 h-24 w-24 rounded-full ${glowColor} blur-xl`} />
    </div>
  );
}

function WorkerStatusBadge({ status }: { status: AdminWorkerNode["status"] }) {
  const tone =
    status === "online"
      ? {
          dot: "bg-[var(--success)]",
          label: "Online",
          text: "text-[var(--success)]",
          title: "Heartbeat received within the last 60 seconds.",
        }
      : status === "stale"
        ? {
            dot: "bg-[var(--accent-bright)]",
            label: "Stale",
            text: "text-[var(--accent-bright)]",
            title: "No heartbeat for more than 60 seconds.",
          }
        : {
            dot: "bg-[var(--error)]",
            label: "Offline",
            text: "text-[var(--error)]",
            title: "No heartbeat for more than 5 minutes.",
          };

  return (
    <span
      className={`inline-flex items-center gap-2 rounded-full border border-[var(--border)] bg-white/72 px-3 py-1 text-xs ${tone.text}`}
      title={tone.title}
    >
      <span className={`h-2.5 w-2.5 rounded-full ${tone.dot}`} />
      {tone.label}
    </span>
  );
}

function WorkerNodeCard({ node, referenceNowMs }: { node: AdminWorkerNode; referenceNowMs: number }) {
  const pythonVersion =
    typeof node.metadata.python_version === "string" ? node.metadata.python_version : null;

  return (
    <article className="surface-elevated rounded-[30px] px-5 py-5">
      <div className="flex items-start justify-between gap-3">
        <div>
          <WorkerStatusBadge status={node.status} />
          <h3 className="mt-3 text-lg font-semibold text-[var(--foreground)]">{node.workerId}</h3>
          <p className="mt-1 text-xs text-[var(--foreground-tertiary)]">
            {node.hostname}
            {node.pid != null ? ` · pid ${node.pid}` : ""}
          </p>
        </div>
        <span className="rounded-full border border-[var(--border)] bg-white/72 px-3 py-1 text-xs text-[var(--foreground-secondary)]">
          {node.slots} slot{node.slots === 1 ? "" : "s"}
        </span>
      </div>

      <dl className="mt-5 grid grid-cols-2 gap-x-4 gap-y-3 text-sm">
        <div>
          <dt className="text-[var(--foreground-tertiary)]">Uptime</dt>
          <dd className="mt-1 font-medium text-[var(--foreground)]">
            {formatRelativeTime(node.startedAt, referenceNowMs)}
          </dd>
        </div>
        <div>
          <dt className="text-[var(--foreground-tertiary)]">Last Seen</dt>
          <dd className="mt-1 font-medium text-[var(--foreground)]">
            {formatRelativeTime(node.lastHeartbeat, referenceNowMs)}
          </dd>
        </div>
      </dl>

      <div className="mt-5 grid grid-cols-3 gap-3">
        {[
          { label: "Active now", value: node.activeJobs, valueClassName: "text-[var(--foreground)]" },
          { label: "Done 24h", value: node.completed24h, valueClassName: "text-[var(--success)]" },
          { label: "Failed 24h", value: node.failed24h, valueClassName: node.failed24h > 0 ? "text-[var(--error)]" : "text-[var(--foreground)]" },
        ].map((metric) => (
          <div
            key={metric.label}
            className="rounded-[20px] border border-[var(--border)] bg-white/60 px-3 py-3"
          >
            <p className={`text-xl font-semibold ${metric.valueClassName}`}>{formatCount(metric.value)}</p>
            <p className="mt-1 text-[11px] text-[var(--foreground-tertiary)]">{metric.label}</p>
          </div>
        ))}
      </div>

      <div className="mt-5 flex flex-wrap items-center justify-between gap-3 text-xs text-[var(--foreground-secondary)]">
        <p>
          Avg job time:{" "}
          <span className="font-medium text-[var(--foreground)]">
            {node.avgDurationMs24h != null ? formatDuration(node.avgDurationMs24h) : "—"}
          </span>
        </p>
        {pythonVersion ? (
          <span className="rounded-full border border-[var(--border)] bg-white/72 px-2.5 py-1">
            Python {pythonVersion}
          </span>
        ) : null}
      </div>
    </article>
  );
}

export function AdminWorkersScreen() {
  const [range, setRange] = useState<AdminRange>("7d");
  const [expandedJob, setExpandedJob] = useState<string | null>(null);
  const [retryingJobId, setRetryingJobId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [workerNodes, setWorkerNodes] = useState<AdminWorkerNode[]>([]);
  const [workerNodesLoading, setWorkerNodesLoading] = useState(true);
  const [workerNodesError, setWorkerNodesError] = useState<string | null>(null);
  const [workerNodesGeneratedAt, setWorkerNodesGeneratedAt] = useState<string>("");
  const { data, error, isLoading, refresh } = useAdminResource({
    range,
    loader: admin.getWorkers,
    errorMessage: "Failed to load worker metrics.",
  });

  const loadWorkerNodes = useCallback(async (options?: { preserveData?: boolean }) => {
    if (!options?.preserveData) {
      setWorkerNodesLoading(true);
    }

    try {
      const result = await admin.getWorkerNodes();
      setWorkerNodes(result.nodes);
      setWorkerNodesGeneratedAt(result.generatedAt);
      setWorkerNodesError(null);
    } catch (nextError) {
      setWorkerNodesError(getApiErrorMessage(nextError, "Failed to load worker nodes."));
    } finally {
      setWorkerNodesLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadWorkerNodes();

    const intervalId = window.setInterval(() => {
      void loadWorkerNodes({ preserveData: true });
    }, WORKER_NODE_POLL_INTERVAL_MS);

    return () => window.clearInterval(intervalId);
  }, [loadWorkerNodes]);

  async function handleRetryJob(jobId: string) {
    setRetryingJobId(jobId);
    setActionError(null);

    try {
      await admin.retryJob(jobId);
      if (expandedJob === jobId) {
        setExpandedJob(null);
      }
      await Promise.all([refresh(), loadWorkerNodes({ preserveData: true })]);
    } catch (nextError) {
      setActionError(getApiErrorMessage(nextError, "Failed to retry job."));
    } finally {
      setRetryingJobId(null);
    }
  }

  const nowMs = Date.now();

  return (
    <AdminLayout
      currentPath="/admin/workers"
      title="Workers"
      description="Worker fleet status, job queue, and failure triage."
      actions={<AdminRangePicker value={range} onChange={setRange} />}
    >
      {isLoading && !data ? (
        <DashboardSkeleton />
      ) : error && !data ? (
        <DashboardState
          action={
            <button className="button-primary" onClick={() => void refresh()} type="button">
              Retry
            </button>
          }
          description={error}
          title="Worker metrics could not be loaded"
          tone="error"
        />
      ) : data ? (
        <div className="space-y-6">
          {error ? (
            <DashboardNotice title="Showing last successful snapshot." description={error} tone="error" />
          ) : null}

          {actionError ? (
            <DashboardNotice title="Job action failed" description={actionError} tone="error" />
          ) : null}

          <section className="surface-elevated rounded-[32px] px-6 py-6">
            <p className="eyebrow">Operator View</p>
            <div className="mt-4 flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
              <div className="max-w-2xl">
                <h2 className="text-2xl font-semibold tracking-[-0.04em] text-[var(--foreground)]">
                  Watch queue pressure, worker node health, and failure recovery from one place.
                </h2>
                <p className="mt-3 text-sm leading-7 text-[var(--foreground-secondary)]">
                  This page stays focused on the worker fleet itself: live jobs, heartbeat visibility,
                  backlog pressure, and fast retry loops for failures that need operator attention.
                </p>
              </div>
              <div className="flex flex-wrap gap-2">
                <span className="admin-chip admin-chip-brand">{data.statusCounts.running} running</span>
                <span className="admin-chip">{data.statusCounts.pending} pending</span>
                <span className="admin-chip admin-chip-warning">
                  {data.metrics.pendingBacklog.current} backlog
                </span>
              </div>
            </div>
          </section>

          <section className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
            <StatCard
              label="Jobs Completed"
              value={data.metrics.jobsCompleted.current}
              accentColor="bg-[var(--brand-bright)]"
              glowColor="bg-[rgba(111, 151, 201,0.18)]"
            >
              {data.metrics.jobsCompleted.delta !== 0 ? (
                <div
                  className={`flex items-center text-xs ${
                    data.metrics.jobsCompleted.delta > 0 ? "text-[var(--success)]" : "text-[var(--error)]"
                  }`}
                >
                  {data.metrics.jobsCompleted.delta > 0 ? (
                    <svg className="mr-1 h-3 w-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path d="M7 11l5-5m0 0l5 5m-5-5v12" strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} />
                    </svg>
                  ) : (
                    <svg className="mr-1 h-3 w-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path d="M17 13l-5 5m0 0l-5-5m5 5V6" strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} />
                    </svg>
                  )}
                  {data.metrics.jobsCompleted.delta > 0 ? "+" : ""}
                  {formatCount(data.metrics.jobsCompleted.delta)} from prev
                </div>
              ) : (
                <div className="text-xs text-[var(--foreground-tertiary)]">No change</div>
              )}
            </StatCard>

            <StatCard
              label="Pending Backlog"
              value={data.metrics.pendingBacklog.current}
              accentColor="bg-[var(--accent-bright)]"
              glowColor="bg-[rgba(212,156,105,0.18)]"
            >
              <div className="text-xs text-[var(--accent-bright)]">Awaiting available worker slots</div>
            </StatCard>

            <StatCard
              label="Active Processing"
              value={data.statusCounts.running}
              accentColor="bg-[var(--foreground)]"
              glowColor="bg-[rgba(36,29,21,0.12)]"
            >
              <div className="flex items-center text-xs text-[var(--foreground-secondary)]">
                {data.statusCounts.running > 0 ? (
                  <>
                    <span className="mr-2 h-1.5 w-1.5 animate-pulse rounded-full bg-[var(--foreground)]" />
                    Live processing
                  </>
                ) : (
                  "Idle"
                )}
              </div>
            </StatCard>

            <StatCard
              label="Failed Jobs"
              value={data.metrics.jobsFailed.current}
              accentColor="bg-[var(--error)]"
              glowColor="bg-[rgba(191,91,70,0.14)]"
            >
              {data.metrics.jobsFailed.current > 0 ? (
                <div className="text-xs text-[var(--error)]">Review recent failures below</div>
              ) : (
                <div className="text-xs text-[var(--foreground-tertiary)]">All clear</div>
              )}
            </StatCard>
          </section>

          <section>
            <div className="mb-4 flex items-center justify-between gap-3">
              <div className="flex items-center gap-3">
                <h2 className="text-xl font-semibold text-[var(--foreground)]">Worker Nodes</h2>
                <span className="admin-chip admin-chip-brand">{workerNodes.length} registered</span>
              </div>
              {workerNodesGeneratedAt ? (
                <span className="text-xs text-[var(--foreground-tertiary)]">
                  Updated {formatAdminDateTime(workerNodesGeneratedAt)}
                </span>
              ) : null}
            </div>

            {workerNodesError && workerNodes.length > 0 ? (
              <DashboardNotice
                title="Worker node refresh failed"
                description={workerNodesError}
                tone="error"
              />
            ) : null}

            {workerNodesLoading && workerNodes.length === 0 ? (
              <div className="surface-elevated rounded-[30px] px-5 py-8 text-sm text-[var(--foreground-tertiary)]">
                Loading worker nodes…
              </div>
            ) : workerNodesError && workerNodes.length === 0 ? (
              <DashboardState
                action={
                  <button className="button-primary" onClick={() => void loadWorkerNodes()} type="button">
                    Retry
                  </button>
                }
                description={workerNodesError}
                title="Worker nodes could not be loaded"
                tone="error"
              />
            ) : workerNodes.length === 0 ? (
              <div className="surface-elevated rounded-[30px] px-5 py-8 text-sm text-[var(--foreground-tertiary)]">
                No worker nodes registered. Workers will appear here once they start sending heartbeats.
              </div>
            ) : (
              <div className="grid gap-3 xl:grid-cols-2">
                {workerNodes.map((node) => (
                  <WorkerNodeCard key={node.workerId} node={node} referenceNowMs={nowMs} />
                ))}
              </div>
            )}
          </section>

          <section>
            <div className="mb-4 flex items-center justify-between">
              <div className="flex items-center gap-3">
                <h2 className="text-xl font-semibold text-[var(--foreground)]">Worker Fleet</h2>
                <span className="admin-chip admin-chip-brand">
                  {data.statusCounts.running} RUNNING / {data.statusCounts.pending} PENDING
                </span>
              </div>
            </div>
            <WorkerLivePanel />
          </section>

          <section>
            <h2 className="mb-4 text-xl font-semibold text-[var(--foreground)]">Recent Failures</h2>
            <div className="surface-elevated overflow-hidden rounded-[30px] px-5 py-5">
              {data.recentFailedJobs.length === 0 ? (
                <p className="py-6 text-sm text-[var(--foreground-tertiary)]">No failures in this window.</p>
              ) : (
                <table className="admin-table text-sm">
                  <thead>
                    <tr>
                      <th>Source</th>
                      <th>URL</th>
                      <th>Error</th>
                      <th>Attempts</th>
                      <th>Time</th>
                      <th>Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.recentFailedJobs.map((job) => {
                      const isExpanded = expandedJob === job.jobId;
                      const videoUrl = buildVideoUrl(job);

                      return (
                        <tr
                          key={job.jobId}
                          className="cursor-pointer"
                          onClick={() => setExpandedJob(isExpanded ? null : job.jobId)}
                        >
                          <td>
                            <p className="admin-table-primary">{formatSourceLabel(job)}</p>
                          </td>
                          <td>
                            {videoUrl ? (
                              <a
                                href={videoUrl}
                                target="_blank"
                                rel="noopener noreferrer"
                                className="block max-w-[16rem] truncate text-[var(--brand-bright)] underline-offset-2 hover:underline"
                                onClick={(event) => event.stopPropagation()}
                                title={videoUrl}
                              >
                                {job.videoId ?? videoUrl}
                              </a>
                            ) : (
                              <span className="text-[var(--foreground-tertiary)]">—</span>
                            )}
                          </td>
                          <td>
                            <div>
                              <span className="inline-flex rounded-full border border-[var(--border)] bg-white/72 px-2 py-0.5 text-[10px] text-[var(--foreground-secondary)]">
                                {job.jobType}
                              </span>
                              <p className="mt-2 truncate text-[var(--error)]" style={{ maxWidth: "22rem" }}>
                                {job.errorMessage ? job.errorMessage.split("\n")[0]?.slice(0, 96) : "Unknown error"}
                              </p>
                              {isExpanded && job.errorMessage ? (
                                <pre className="mt-2 max-w-xl overflow-x-auto whitespace-pre-wrap break-all rounded-lg border border-[rgba(191,91,70,0.18)] bg-[rgba(191,91,70,0.1)] p-3 text-[10px] leading-5 text-[var(--error)]">
                                  {job.errorMessage}
                                </pre>
                              ) : null}
                            </div>
                          </td>
                          <td className="whitespace-nowrap">
                            {job.attempts}/{job.maxAttempts}
                          </td>
                          <td className="whitespace-nowrap">{formatAdminDateTime(job.updatedAt)}</td>
                          <td>
                            <button
                              className="inline-flex items-center gap-1 rounded-full border border-[var(--border)] bg-white/72 px-3 py-1 text-xs text-[var(--foreground-secondary)] transition hover:border-[var(--border-strong)] hover:bg-white hover:text-[var(--foreground)] disabled:cursor-not-allowed disabled:opacity-60"
                              type="button"
                              disabled={retryingJobId === job.jobId}
                              onClick={(event) => {
                                event.stopPropagation();
                                void handleRetryJob(job.jobId);
                              }}
                            >
                              <svg className="h-3.5 w-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path
                                  d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182"
                                  strokeLinecap="round"
                                  strokeLinejoin="round"
                                  strokeWidth={2}
                                />
                              </svg>
                              {retryingJobId === job.jobId ? "Retrying..." : "Retry"}
                            </button>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              )}
            </div>
          </section>
        </div>
      ) : (
        <DashboardState
          action={
            <button className="button-primary" onClick={() => void refresh()} type="button">
              Retry
            </button>
          }
          description="No worker payload returned."
          title="No data available"
        />
      )}
    </AdminLayout>
  );
}
