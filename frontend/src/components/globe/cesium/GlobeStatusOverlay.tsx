import { useMemo } from "react";
import type { Event } from "@/types/event";
import { isQuake, isWeather } from "@/components/globe/layers";

interface GlobeStatusOverlayProps {
  events: Event[];
  weatherEvents?: Event[];
  globeTotal?: number;
  globeWeatherTotal?: number;
  globeLoading?: boolean;
  globeWeatherLoading?: boolean;
  globeLoadingMore?: boolean;
}

/**
 * Bottom-left globe status pill: quake load progress plus active weather alerts.
 */
export function GlobeStatusOverlay({
  events,
  weatherEvents = [],
  globeTotal,
  globeWeatherTotal,
  globeLoading,
  globeWeatherLoading,
  globeLoadingMore,
}: GlobeStatusOverlayProps) {
  const quakeCount = events.filter(isQuake).length;
  const weatherCount = weatherEvents.filter(isWeather).length;
  const total = Math.max(globeTotal ?? 0, quakeCount);
  const weatherTotal = Math.max(globeWeatherTotal ?? 0, weatherCount);
  const stillLoading = Boolean(globeLoading && quakeCount === 0);
  const loadingMore = Boolean(globeLoadingMore);
  const partial = total > quakeCount;

  const statusLabel = useMemo(() => {
    const parts: string[] = [];

    if (stillLoading) {
      parts.push(
        total > 0
          ? `Loading quakes… ${quakeCount.toLocaleString()} / ${total.toLocaleString()}`
          : "Loading quakes…",
      );
    } else if (quakeCount === 0) {
      parts.push("No quakes match filters");
    } else if (loadingMore || partial) {
      parts.push(
        `${quakeCount.toLocaleString()} / ${total.toLocaleString()} quakes${loadingMore ? "…" : ""}`,
      );
    } else {
      parts.push(`${total.toLocaleString()} quake${total === 1 ? "" : "s"}`);
    }

    if (globeWeatherLoading && weatherCount === 0) {
      parts.push("loading weather…");
    } else if (weatherTotal > 0 || weatherCount > 0) {
      const count = Math.max(weatherTotal, weatherCount);
      parts.push(`${count.toLocaleString()} alert${count === 1 ? "" : "s"}`);
    }

    return parts.join(" · ");
  }, [
    stillLoading,
    total,
    quakeCount,
    loadingMore,
    partial,
    globeWeatherLoading,
    weatherCount,
    weatherTotal,
  ]);

  return (
    <div className="glass-panel pointer-events-none absolute bottom-4 left-4 rounded-full px-3 py-1.5 text-xs text-foreground/55">
      {statusLabel}
    </div>
  );
}
