import type { ReactNode } from "react";
import type { Event } from "@/types/event";
import { cn } from "@/lib/utils";

const SEVERITY_LABEL: Record<Event["severity"], string> = {
  info: "Info",
  low: "Low",
  moderate: "Moderate",
  high: "High",
  critical: "Critical",
};

const SEVERITY_DOT: Record<Event["severity"], string> = {
  info: "bg-foreground/40",
  low: "bg-sky-400",
  moderate: "bg-primary",
  high: "bg-orange-500",
  critical: "bg-destructive",
};

interface EventListProps {
  events: Event[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}

export function EventList({ events, selectedId, onSelect }: EventListProps) {
  if (events.length === 0) {
    return (
      <p className="px-4 py-6 text-sm leading-relaxed text-foreground/50">
        No events yet. Live ingest will appear here when workers index data for your tenant.
      </p>
    );
  }

  return (
    <ul className="flex flex-col gap-0.5 p-2">
      {events.map((event) => {
        const selected = event.id === selectedId;
        return (
          <li key={event.id}>
            <button
              type="button"
              className={cn(
                "w-full rounded-xl px-3 py-2.5 text-left text-sm transition-colors hover:bg-white/10",
                selected && "bg-white/15 ring-1 ring-white/15",
              )}
              onClick={() => onSelect(event.id)}
            >
              <div className="flex items-center gap-2">
                <span
                  className={cn("size-2 shrink-0 rounded-full", SEVERITY_DOT[event.severity])}
                  aria-hidden
                />
                <span className="line-clamp-1 flex-1 font-medium text-foreground/90">
                  {event.title ?? event.place_name ?? event.source_event_id}
                </span>
                <span className="shrink-0 text-xs font-semibold tabular-nums text-primary">
                  {event.impact_score}
                </span>
              </div>
              <div className="mt-1 flex gap-2 pl-4 text-xs text-foreground/45">
                <span className="capitalize">{event.category}</span>
                <span>·</span>
                <span>{SEVERITY_LABEL[event.severity]}</span>
              </div>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

interface EventDetailProps {
  event: Event | null;
}

export function EventDetail({ event }: EventDetailProps) {
  if (!event) {
    return (
      <p className="px-4 py-6 text-sm text-foreground/50">Select an event to inspect details.</p>
    );
  }

  return (
    <div className="flex flex-col gap-4 p-4 text-sm">
      <div className="flex flex-col gap-1">
        <div className="flex items-center gap-2">
          <span
            className={cn("size-2.5 shrink-0 rounded-full", SEVERITY_DOT[event.severity])}
            aria-hidden
          />
          <h2 className="text-base font-semibold text-foreground">
            {event.title ?? "Untitled event"}
          </h2>
        </div>
        <p className="text-xs text-foreground/45">
          {event.source} · {new Date(event.occurred_at).toLocaleString()}
        </p>
      </div>
      {event.summary ? (
        <p className="leading-relaxed text-foreground/70">{event.summary}</p>
      ) : null}
      <dl className="grid grid-cols-2 gap-x-4 gap-y-3 text-xs">
        <DetailField label="Category" value={<span className="capitalize">{event.category}</span>} />
        <DetailField label="Severity" value={SEVERITY_LABEL[event.severity]} />
        <DetailField label="Impact" value={String(event.impact_score)} />
        <DetailField
          label="Location"
          value={`${event.location.lat.toFixed(2)}, ${event.location.lon.toFixed(2)}`}
        />
        {event.place_name ? (
          <div className="col-span-2">
            <DetailField label="Place" value={event.place_name} />
          </div>
        ) : null}
      </dl>
    </div>
  );
}

function DetailField({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-[10px] font-medium uppercase tracking-[0.12em] text-foreground/40">
        {label}
      </dt>
      <dd className="text-foreground/85">{value}</dd>
    </div>
  );
}
