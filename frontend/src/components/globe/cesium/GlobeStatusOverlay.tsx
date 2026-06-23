import { useMemo } from "react";
import type { Event } from "@/types/event";
import { isQuake } from "@/components/globe/layers";

interface GlobeStatusOverlayProps {
  events: Event[];
  globeTotal?: number;
  globeLoading?: boolean;
  globeLoadingMore?: boolean;
}

/**
 * Bottom-left quake count / loading pill, preserving the r3f globe's status text
 * (the Cesium credit container occupies the bottom-right).
 */
export function GlobeStatusOverlay({
  events,
  globeTotal,
  globeLoading,
  globeLoadingMore,
}: GlobeStatusOverlayProps) {
  const quakeCount = events.filter(isQuake).length;
  const total = Math.max(globeTotal ?? 0, quakeCount);
  const stillLoading = Boolean(globeLoading && quakeCount === 0);
  const loadingMore = Boolean(globeLoadingMore);
  const partial = total > quakeCount;

  const statusLabel = useMemo(() => {
    if (stillLoading) {
      return total > 0
        ? `Loading quakes… ${quakeCount.toLocaleString()} / ${total.toLocaleString()}`
        : "Loading quakes…";
    }
    if (quakeCount === 0) {
      return "No quakes match filters";
    }
    if (loadingMore || partial) {
      return `${quakeCount.toLocaleString()} / ${total.toLocaleString()} quakes${loadingMore ? "…" : ""}`;
    }
    return `${total.toLocaleString()} quake${total === 1 ? "" : "s"}`;
  }, [stillLoading, total, quakeCount, loadingMore, partial]);

  return (
    <div className="glass-panel pointer-events-none absolute bottom-4 left-4 rounded-full px-3 py-1.5 text-xs text-foreground/55">
      {statusLabel}
    </div>
  );
}
