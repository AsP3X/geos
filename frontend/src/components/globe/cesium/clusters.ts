import { Color } from "cesium";
import type { Event } from "@/types/event";
import { severityToCesiumColor } from "@/components/globe/cesium/severity-colors";

const DEG2RAD = Math.PI / 180;

/**
 * Pre-binned cluster resolutions (cell size in degrees, coarse → fine), ported
 * verbatim from the r3f `useScreenClusters.ts` so clustering density matches the
 * previous globe. Zooming selects the level whose cells project near
 * {@link TARGET_CELL_PX} on screen.
 */
const LEVEL_CELL_DEG = [
  60, 40, 26, 17, 11, 7, 4.5, 3, 2, 1.3, 0.85, 0.55, 0.36, 0.24, 0.16, 0.1, 0.06, 0.035,
];

/** Desired on-screen cell size (CSS px) used to pick the active LOD level. */
export const TARGET_CELL_PX = 26;

/** Member quakes retained per cluster (sorted by impact) for the sidebar list. */
const MEMBER_CAP = 1500;

const SEVERITY_RANK: Record<Event["severity"], number> = {
  info: 0,
  low: 1,
  moderate: 2,
  high: 3,
  critical: 4,
};

/** A merged group of nearby quakes rendered as one labeled bubble. */
export interface GlobeCluster {
  id: string;
  lon: number;
  lat: number;
  count: number;
  /** Member quakes (highest-impact first), capped at {@link MEMBER_CAP}. */
  members: Event[];
  color: Color;
  severity: Event["severity"];
}

export interface ScreenClusters {
  /** Cells holding exactly one quake — rendered as ordinary dots. */
  singles: Event[];
  /** Cells holding 2+ quakes — rendered as labeled bubbles. */
  clusters: GlobeCluster[];
}

interface Cell {
  count: number;
  sumLon: number;
  sumLat: number;
  first: Event;
  members: Event[];
  topSeverity: number;
}

export const EMPTY_CLUSTERS: ScreenClusters = { singles: [], clusters: [] };

/**
 * Bin every event into a (roughly equal-area) lat/lon grid of the given cell
 * size and materialize singles + clusters. Longitude cell width widens toward
 * the poles by 1/cos(lat) so cells stay roughly square. Pure function of the
 * event set + cell size (cacheable per level).
 */
function buildLevel(events: Event[], cellDeg: number): ScreenClusters {
  const cells = new Map<string, Cell>();

  for (let i = 0; i < events.length; i += 1) {
    const event = events[i];
    const { lat, lon } = event.location;
    const cosLat = Math.max(Math.cos(lat * DEG2RAD), 0.15);
    const lonCellDeg = cellDeg / cosLat;
    const cellX = Math.floor((lon + 180) / lonCellDeg);
    const cellY = Math.floor((lat + 90) / cellDeg);
    const key = `${cellY}:${cellX}`;
    const rank = SEVERITY_RANK[event.severity];

    const existing = cells.get(key);
    if (existing) {
      existing.count += 1;
      existing.sumLon += lon;
      existing.sumLat += lat;
      if (existing.members.length < MEMBER_CAP) {
        existing.members.push(event);
      }
      if (rank > existing.topSeverity) {
        existing.topSeverity = rank;
      }
    } else {
      cells.set(key, {
        count: 1,
        sumLon: lon,
        sumLat: lat,
        first: event,
        members: [event],
        topSeverity: rank,
      });
    }
  }

  const singles: Event[] = [];
  const clusters: GlobeCluster[] = [];
  for (const cell of cells.values()) {
    if (cell.count === 1) {
      singles.push(cell.first);
      continue;
    }
    const members = cell.members.slice().sort((a, b) => b.impact_score - a.impact_score);
    const topSeverity = members[0]?.severity ?? "info";
    clusters.push({
      id: `cl_${members[0]?.id ?? cell.count}_${cell.count}`,
      lon: cell.sumLon / cell.count,
      lat: cell.sumLat / cell.count,
      count: cell.count,
      members,
      color: severityToCesiumColor(topSeverity),
      severity: topSeverity,
    });
  }

  return { singles, clusters };
}

/** Pick the precomputed level whose cell size best matches the target px size. */
function pickLevelIndex(desiredDeg: number): number {
  let best = 0;
  let bestErr = Infinity;
  for (let i = 0; i < LEVEL_CELL_DEG.length; i += 1) {
    const err = Math.abs(Math.log(LEVEL_CELL_DEG[i] / desiredDeg));
    if (err < bestErr) {
      bestErr = err;
      best = i;
    }
  }
  return best;
}

/**
 * Caches the merged spheres per LOD level for one event set. Recomputing on
 * camera move only re-selects the level (cheap) unless the event set changed.
 */
export class ClusterIndex {
  private readonly events: Event[];
  private readonly cache = new Map<number, ScreenClusters>();

  constructor(events: Event[]) {
    this.events = events;
  }

  /**
   * Convert the desired ground cell size (in degrees) to a cached level and
   * return its singles + clusters. Returns the same object for repeat calls at
   * the same level so the caller can skip rebuilding Cesium primitives.
   */
  forDesiredDeg(desiredDeg: number): { level: number; result: ScreenClusters } {
    const level = pickLevelIndex(desiredDeg);
    let result = this.cache.get(level);
    if (!result) {
      result = buildLevel(this.events, LEVEL_CELL_DEG[level]);
      this.cache.set(level, result);
    }
    return { level, result };
  }
}

/**
 * Derive the target cluster cell size (degrees) from the Cesium camera height
 * and canvas, mirroring the r3f screen-space heuristic: a cell should subtend
 * roughly {@link TARGET_CELL_PX} pixels.
 */
export function desiredCellDeg(
  cameraHeightMeters: number,
  fovyRadians: number,
  canvasHeightPx: number,
): number {
  if (canvasHeightPx <= 0) {
    return LEVEL_CELL_DEG[0];
  }
  // Vertical ground extent under the camera, then meters-per-pixel.
  const groundExtentMeters = 2 * cameraHeightMeters * Math.tan(fovyRadians / 2);
  const metersPerPixel = groundExtentMeters / canvasHeightPx;
  const cellMeters = TARGET_CELL_PX * metersPerPixel;
  // ~111.32 km per degree of latitude at the surface.
  return cellMeters / 111_320;
}
