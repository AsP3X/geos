import { Color } from "cesium";
import type { Event } from "@/types/event";
import { severityToCesiumColor } from "@/components/globe/cesium/severity-colors";
import {
  LEVEL_CELL_DEG,
  UNCLUSTER_CELL_DEG,
  UNCLUSTER_LEVEL,
  pickLevelIndexWithHysteresis,
} from "@/components/globe/cesium/lod";

// Re-exported for existing call sites that import them from this module.
export { TARGET_CELL_PX, UNCLUSTER_CELL_DEG, desiredCellDeg } from "@/components/globe/cesium/lod";

const DEG2RAD = Math.PI / 180;

/** Member quakes retained per cluster (sorted by impact) for the sidebar list. */
const MEMBER_CAP = 1500;

const SEVERITY_RANK: Record<Event["severity"], number> = {
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
  /** Member quakes (highest-impact first), capped at {@link MEMBER_CAP}. */
  members: Event[];
  color: Color;
  severity: Event["severity"];
}

export interface ScreenClusters {
  /** Cells holding exactly one quake — rendered as ordinary dots. */
  singles: Event[];
  /** Cells holding 2+ quakes — rendered as a white-ringed cluster dot. */
  clusters: GlobeCluster[];
}

interface Cell {
  count: number;
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
      if (existing.members.length < MEMBER_CAP) {
        existing.members.push(event);
      }
      if (rank > existing.topSeverity) {
        existing.topSeverity = rank;
      }
    } else {
      cells.set(key, {
        count: 1,
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
    const anchor = members[0];
    const topSeverity = anchor?.severity ?? "info";
    clusters.push({
      id: `cl_${anchor?.id ?? cell.count}_${cell.count}`,
      // Anchor on the highest-impact member so the marker sits on a real quake,
      // not a geometric centroid that can drift between zoom levels.
      lon: anchor?.location.lon ?? 0,
      lat: anchor?.location.lat ?? 0,
      count: cell.count,
      members,
      color: severityToCesiumColor(topSeverity),
      severity: topSeverity,
    });
  }

  return { singles, clusters };
}

/**
 * Caches the merged spheres per LOD level for one event set. Recomputing on
 * camera move only re-selects the level (cheap) unless the event set changed.
 */
export class ClusterIndex {
  private readonly events: Event[];
  private readonly cache = new Map<number, ScreenClusters>();
  private unclustered: ScreenClusters | null = null;
  private currentLevel: number | null = null;

  constructor(events: Event[]) {
    this.events = events;
  }

  /**
   * Convert the desired ground cell size (in degrees) to a cached level and
   * return its singles + clusters. Returns the same object for repeat calls at
   * the same level so the caller can skip rebuilding Cesium primitives.
   */
  forDesiredDeg(desiredDeg: number): { level: number; result: ScreenClusters } {
    if (this.events.length === 0) {
      return { level: 0, result: EMPTY_CLUSTERS };
    }

    if (desiredDeg <= UNCLUSTER_CELL_DEG) {
      this.currentLevel = UNCLUSTER_LEVEL;
      if (!this.unclustered) {
        this.unclustered = { singles: this.events, clusters: [] };
      }
      return { level: UNCLUSTER_LEVEL, result: this.unclustered };
    }

    const level = pickLevelIndexWithHysteresis(desiredDeg, this.currentLevel);
    this.currentLevel = level;
    let result = this.cache.get(level);
    if (!result) {
      result = buildLevel(this.events, LEVEL_CELL_DEG[level]);
      this.cache.set(level, result);
    }
    return { level, result };
  }
}
