import type { GlobeBBox } from "@/lib/events-api";

/** Fraction the fetched bbox is grown beyond the view so small pans don't refetch. */
export const VIEWPORT_MARGIN = 0.15;

/** Views wider than this (degrees) load globally (null) instead of by bbox. */
export const WHOLE_GLOBE_WIDTH_DEG = 350;

/** Per-edge move (relative to bbox span) below which a refetch is suppressed. */
export const BBOX_CHANGE_TOLERANCE_RATIO = 0.12;

function clampLat(value: number): number {
  return Math.max(-90, Math.min(90, value));
}

function clampLon(value: number): number {
  return Math.max(-180, Math.min(180, value));
}

/**
 * Convert camera view bounds (WGS84 degrees) into a margin-expanded fetch bbox.
 *
 * Returns `null` for whole-globe or antimeridian-wrapping views, signalling the
 * caller to load globally — a single API envelope (`ST_MakeEnvelope`) cannot
 * express a bbox that wraps the antimeridian, and a near-global view has no
 * benefit from scoping.
 */
export function viewBoundsToBBox(
  westDeg: number,
  southDeg: number,
  eastDeg: number,
  northDeg: number,
): GlobeBBox | null {
  // east <= west means the view wraps the antimeridian.
  if (eastDeg <= westDeg) {
    return null;
  }
  const widthDeg = eastDeg - westDeg;
  if (widthDeg >= WHOLE_GLOBE_WIDTH_DEG) {
    return null;
  }
  const heightDeg = Math.max(northDeg - southDeg, 0);
  const marginLon = widthDeg * VIEWPORT_MARGIN;
  const marginLat = heightDeg * VIEWPORT_MARGIN;
  return {
    minLon: clampLon(westDeg - marginLon),
    minLat: clampLat(southDeg - marginLat),
    maxLon: clampLon(eastDeg + marginLon),
    maxLat: clampLat(northDeg + marginLat),
  };
}

/**
 * True when the viewport changed enough (vs the last fetched bbox) to warrant a
 * refetch. `null <-> bbox` transitions always count; `null <-> null` (staying
 * global) never does; otherwise any edge must move beyond a span-relative
 * tolerance so tiny pans/zoom jitter don't trigger network churn.
 */
export function bboxChanged(prev: GlobeBBox | null, next: GlobeBBox | null): boolean {
  if (prev === null || next === null) {
    return prev !== next;
  }
  const tol =
    Math.max(next.maxLon - next.minLon, next.maxLat - next.minLat) * BBOX_CHANGE_TOLERANCE_RATIO;
  return (
    Math.abs(prev.minLon - next.minLon) > tol ||
    Math.abs(prev.minLat - next.minLat) > tol ||
    Math.abs(prev.maxLon - next.maxLon) > tol ||
    Math.abs(prev.maxLat - next.maxLat) > tol
  );
}
