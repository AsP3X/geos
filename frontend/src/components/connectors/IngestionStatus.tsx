import { CheckCircle2, Loader2 } from "lucide-react";
import type { ConnectorStatus } from "@/lib/connectors-api";

const COUNT_FORMAT = new Intl.NumberFormat();

function formatCount(value: number): string {
  return COUNT_FORMAT.format(value);
}

function formatDate(value?: string | null): string | null {
  if (!value) {
    return null;
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return null;
  }
  return date.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

function formatTime(value?: string | Date | null): string | null {
  if (!value) {
    return null;
  }
  const date = value instanceof Date ? value : new Date(value);
  if (Number.isNaN(date.getTime())) {
    return null;
  }
  return date.toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
    second: "2-digit",
  });
}

function formatStatusLabel(connector: ConnectorStatus): string {
  const { backfill } = connector;
  if (!connector.enabled) {
    return "Disabled";
  }
  if (backfill.complete) {
    return "Complete";
  }
  if (!backfill.initialized) {
    return "Starting…";
  }
  const cursor = formatDate(backfill.cursor);
  if (cursor) {
    return `Back to ${cursor}`;
  }
  return "Pulling…";
}

function ConnectorRow({
  connector,
  eventsDelta,
}: {
  connector: ConnectorStatus;
  eventsDelta: number;
}) {
  const { backfill } = connector;
  const target = formatDate(backfill.window_start);
  const inProgress = connector.enabled && backfill.initialized && !backfill.complete;
  const total = backfill.total_estimate ?? null;
  // Only treat the total as a denominator once the server says counting finished.
  const hasTotal = backfill.total_final === true && total !== null && total > 0;
  const counting = backfill.counting === true;
  const percent = Math.min(100, Math.max(0, Math.round(backfill.fraction * 100)));

  return (
    <li className="px-4 py-3">
      <div className="flex items-center justify-between gap-2">
        <span className="truncate text-sm font-medium text-foreground/90">
          {connector.source_name}
        </span>
        <span className="inline-flex shrink-0 items-center gap-1 text-xs text-foreground/55">
          {backfill.complete ? (
            <CheckCircle2 size={13} className="text-primary" />
          ) : inProgress ? (
            <Loader2 size={13} className="animate-spin text-primary/80" />
          ) : null}
          {formatStatusLabel(connector)}
        </span>
      </div>

      {backfill.complete ? (
        <div className="mt-2 h-1.5 w-full overflow-hidden rounded-full bg-white/10">
          <div className="h-full w-full rounded-full bg-primary" />
        </div>
      ) : inProgress && hasTotal ? (
        <div className="mt-2 h-1.5 w-full overflow-hidden rounded-full bg-white/10">
          <div
            className="h-full rounded-full bg-primary transition-[width] duration-500 ease-out"
            style={{ width: `${Math.max(percent, 1)}%` }}
          />
        </div>
      ) : inProgress ? (
        <div className="mt-2 h-1.5 w-full overflow-hidden rounded-full bg-white/10">
          <div className="h-full w-1/3 animate-pulse rounded-full bg-primary/80" />
        </div>
      ) : null}

      <div className="mt-1.5 space-y-0.5 text-[11px] text-foreground/50">
        <div className="tabular-nums">
          {hasTotal ? (
            <>
              {formatCount(backfill.events_ingested)} / {formatCount(total)} catalog quakes ({percent}
              %)
            </>
          ) : counting ? (
            <>
              {formatCount(backfill.events_ingested)} pulled · counting catalog
              {total && total > 0 ? ` (est. ${formatCount(total)}+)` : "…"}
            </>
          ) : (
            <>{formatCount(backfill.events_ingested)} events pulled</>
          )}
          {eventsDelta > 0 ? (
            <span className="ml-1 text-primary/90">+{formatCount(eventsDelta)}</span>
          ) : null}
        </div>
        {target ? (
          <div className="text-foreground/45">
            {backfill.complete ? `Since ${target}` : `Target ${target}`}
          </div>
        ) : null}
      </div>
    </li>
  );
}

interface IngestionStatusProps {
  connectors: ConnectorStatus[];
  loading: boolean;
  error: string | null;
  updatedAt: Date | null;
  deltas: Record<string, { eventsDelta: number }>;
}

function latestProgressUpdate(connectors: ConnectorStatus[]): Date | null {
  let latest: Date | null = null;
  for (const connector of connectors) {
    const raw = connector.backfill.progress_updated_at;
    if (!raw) {
      continue;
    }
    const date = new Date(raw);
    if (Number.isNaN(date.getTime())) {
      continue;
    }
    if (!latest || date > latest) {
      latest = date;
    }
  }
  return latest;
}

/** Per-source ingestion + historical backfill progress list. */
export function IngestionStatus({
  connectors,
  loading,
  error,
  updatedAt,
  deltas,
}: IngestionStatusProps) {
  if (error && connectors.length === 0) {
    return <p className="p-4 text-sm text-destructive">{error}</p>;
  }
  if (loading && connectors.length === 0) {
    return <p className="p-4 text-sm text-foreground/50">Loading ingestion status…</p>;
  }
  if (connectors.length === 0) {
    return <p className="p-4 text-sm text-foreground/50">No connectors registered.</p>;
  }

  const progressUpdatedAt = latestProgressUpdate(connectors) ?? updatedAt;
  const progressTime = formatTime(progressUpdatedAt);

  return (
    <div>
      {error ? <p className="border-b border-destructive/30 px-4 py-2 text-xs text-destructive">{error}</p> : null}
      <ul className="divide-y divide-white/5">
        {connectors.map((connector) => (
          <ConnectorRow
            key={connector.source_key}
            connector={connector}
            eventsDelta={deltas[connector.source_key]?.eventsDelta ?? 0}
          />
        ))}
      </ul>
      {progressTime ? (
        <p className="border-t border-white/5 px-4 py-2 text-[10px] text-foreground/40">
          Progress updated {progressTime}
        </p>
      ) : null}
    </div>
  );
}
