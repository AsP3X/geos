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

/** Max instanced dot markers rendered at once (heatmap uses the full set). */
export const MARKER_DISPLAY_CAP = 8192;

/**
 * Downsample quakes for the dot layer when the filtered set exceeds the GPU
 * cap. Keeps the highest-impact events so significant quakes stay visible.
 */
export function sampleMarkerEvents(
  events: Event[],
  cap = MARKER_DISPLAY_CAP,
): Event[] {
  if (events.length <= cap) {
    return events;
  }
  return [...events]
    .sort((a, b) => b.impact_score - a.impact_score)
    .slice(0, cap);
}

/**
 * Compact fingerprint of an event list so globe layers remount when the visible
 * set changes (filter swap, live upsert, pagination) even if length stays equal.
 */
export function eventsFingerprint(events: Event[]): string {
  if (events.length === 0) {
    return "0";
  }
  if (events.length <= 128) {
    return events.map((event) => event.id).join("|");
  }
  let hash = events.length;
  for (const event of events) {
    hash = (hash * 31 + event.id.charCodeAt(0)) | 0;
  }
  return `${events.length}|${events[0].id}|${events[events.length - 1].id}|${hash}`;
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
