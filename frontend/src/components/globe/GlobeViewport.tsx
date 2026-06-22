import type { Event } from "@/types/event";
import { cn } from "@/lib/utils";

const SEVERITY_COLOR: Record<Event["severity"], string> = {
  info: "bg-muted-foreground",
  low: "bg-sky-400",
  moderate: "bg-primary",
  high: "bg-orange-500",
  critical: "bg-destructive",
};

function project(lon: number, lat: number): { x: number; y: number } {
  const x = ((lon + 180) / 360) * 100;
  const y = ((90 - lat) / 180) * 100;
  return { x, y };
}

interface GlobeViewportProps {
  events: Event[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}

/** Placeholder 2D map until react-three-fiber globe lands. Fills its container edge-to-edge. */
export function GlobeViewport({ events, selectedId, onSelect }: GlobeViewportProps) {
  return (
    <div className="relative size-full min-h-full overflow-hidden bg-[radial-gradient(ellipse_at_center,oklch(0.28_0.04_240)_0%,oklch(0.18_0.02_260)_45%,oklch(0.12_0.01_60)_100%)]">
      <div className="pointer-events-none absolute inset-0 opacity-30 [background-image:linear-gradient(oklch(0.45_0.02_240_/_0.35)_1px,transparent_1px),linear-gradient(90deg,oklch(0.45_0.02_240_/_0.35)_1px,transparent_1px)] [background-size:48px_48px]" />
      {events.map((event) => {
        const { x, y } = project(event.location.lon, event.location.lat);
        const selected = event.id === selectedId;
        return (
          <button
            key={event.id}
            type="button"
            aria-label={event.title ?? event.source_event_id}
            className={cn(
              "absolute size-3 -translate-x-1/2 -translate-y-1/2 rounded-full border border-background shadow-sm transition-transform hover:scale-125",
              SEVERITY_COLOR[event.severity],
              selected && "ring-2 ring-primary ring-offset-2 ring-offset-background",
            )}
            style={{ left: `${x}%`, top: `${y}%` }}
            onClick={() => onSelect(event.id)}
          />
        );
      })}
      <div className="glass-panel pointer-events-none absolute bottom-4 left-4 rounded-full px-3 py-1.5 text-xs text-foreground/55">
        Globe scaffold — {events.length} events
      </div>
    </div>
  );
}
