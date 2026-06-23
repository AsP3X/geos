import type { Event } from "@/types/event";

/**
 * Visualization layers rendered on the globe. Each flag is purely visual:
 * toggling a layer only shows/hides its markers on the globe — events stay
 * loaded and continue to appear in the event list, search, and detail panel.
 */
export interface GlobeLayers {
  /** Earthquake point markers (where quakes occurred). */
  quakeDots: boolean;
  /** Earthquake heat field (intensity scaled by magnitude). */
  quakeHeat: boolean;
  /** Weather layer — placeholder, not yet implemented. */
  weather: boolean;
}

export const DEFAULT_LAYERS: GlobeLayers = {
  quakeDots: true,
  quakeHeat: false,
  weather: false,
};

/** Events that belong to the quake layers. */
export function isQuake(event: Event): boolean {
  return event.category === "earthquake";
}

/**
 * Normalized 0–1 "strength" used to size and color a quake on the heatmap.
 * Prefers seismic magnitude (capped at M8) and falls back to the normalized
 * impact score when a source omits magnitude.
 */
export function quakeStrength(event: Event): number {
  const value =
    event.magnitude != null && Number.isFinite(event.magnitude)
      ? event.magnitude / 8
      : event.impact_score / 100;
  return Math.min(Math.max(value, 0), 1);
}
