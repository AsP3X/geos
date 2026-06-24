import { Color } from "cesium";
import type { Event } from "@/types/event";

/**
 * Cluster marker types shared between the clustering worker (which returns
 * indices + ranks over typed arrays) and the renderer (which maps them back to
 * `Event` objects). The grid/culling logic itself lives in `cluster-grid.ts`.
 */

/** Severity tiers ordered low → high; the array index is the numeric rank. */
export const SEVERITY_ORDER: readonly Event["severity"][] = [
  "info",
  "low",
  "moderate",
  "high",
  "critical",
];

/** Severity → numeric rank (inverse of {@link SEVERITY_ORDER}). */
export const SEVERITY_RANK: Record<Event["severity"], number> = {
  info: 0,
  low: 1,
  moderate: 2,
  high: 3,
  critical: 4,
};

/** A merged group of nearby quakes rendered as a white-ringed dot. */
export interface GlobeCluster {
  id: string;
  lon: number;
  lat: number;
  count: number;
  /** Member quakes (highest-impact first), capped by the clustering worker. */
  members: Event[];
  color: Color;
  severity: Event["severity"];
}

export interface ScreenClusters {
  /** Events rendered as ordinary single dots. */
  singles: Event[];
  /** Cells holding 2+ quakes, rendered as ringed cluster dots. */
  clusters: GlobeCluster[];
}
