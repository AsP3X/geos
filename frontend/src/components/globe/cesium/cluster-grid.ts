/**
 * Pure grid clustering + viewport culling over parallel typed arrays.
 *
 * Deliberately free of any `cesium`/DOM import so it can run in a Web Worker and
 * be unit-tested under plain Node. The worker (`clusters.worker.ts`) holds the
 * dataset and uses these functions; the main thread maps the returned indices
 * back to `Event` objects for rendering and picking.
 */

const DEG2RAD = Math.PI / 180;

/** Member quakes retained per cluster (highest-impact first) for the sidebar list. */
export const CLUSTER_MEMBER_CAP = 1500;

/** Parallel-array view of the loaded quakes (index `i` is one event). */
export interface ClusterDataset {
  lon: Float64Array;
  lat: Float64Array;
  /** Severity tier as a 0..n rank (see `SEVERITY_RANK` on the main thread). */
  severityRank: Uint8Array;
  impact: Float32Array;
  count: number;
}

/** Geographic view bounds in WGS84 degrees (non-wrapping: `east > west`). */
export interface ViewRect {
  west: number;
  south: number;
  east: number;
  north: number;
}

/** A merged group of nearby quakes. Member indices point into the dataset. */
export interface GridCluster {
  /** Stable per (level, cell); lets the renderer diff clusters across pans. */
  cellKey: string;
  lon: number;
  lat: number;
  count: number;
  /** Severity rank of the anchor (highest-impact member). */
  severityRank: number;
  /** Member indices, highest-impact first, capped at {@link CLUSTER_MEMBER_CAP}. */
  memberIndices: number[];
}

export interface GridResult {
  /** Indices of events alone in their cell — rendered as ordinary dots. */
  singleIndices: number[];
  /** Cells holding 2+ quakes — rendered as a ringed cluster dot. */
  clusters: GridCluster[];
}

/**
 * Bin every event into a (roughly equal-area) lat/lon grid of the given cell
 * size. Longitude cell width widens toward the poles by 1/cos(lat) so cells stay
 * roughly square. View-independent and cacheable per cell size.
 *
 * When `uncluster` is set (deep zoom), skip binning and treat every event as its
 * own dot.
 */
export function buildFullGrid(
  data: ClusterDataset,
  cellDeg: number,
  uncluster: boolean,
): GridResult {
  if (uncluster) {
    const singleIndices = new Array<number>(data.count);
    for (let i = 0; i < data.count; i += 1) {
      singleIndices[i] = i;
    }
    return { singleIndices, clusters: [] };
  }

  const cells = new Map<string, { count: number; members: number[] }>();
  for (let i = 0; i < data.count; i += 1) {
    const lat = data.lat[i];
    const lon = data.lon[i];
    const cosLat = Math.max(Math.cos(lat * DEG2RAD), 0.15);
    const lonCellDeg = cellDeg / cosLat;
    const cellX = Math.floor((lon + 180) / lonCellDeg);
    const cellY = Math.floor((lat + 90) / cellDeg);
    const key = `${cellY}:${cellX}`;
    const existing = cells.get(key);
    if (existing) {
      existing.count += 1;
      if (existing.members.length < CLUSTER_MEMBER_CAP) {
        existing.members.push(i);
      }
    } else {
      cells.set(key, { count: 1, members: [i] });
    }
  }

  const singleIndices: number[] = [];
  const clusters: GridCluster[] = [];
  for (const [cellKey, cell] of cells) {
    if (cell.count === 1) {
      singleIndices.push(cell.members[0]);
      continue;
    }
    // Anchor on the highest-impact member so the marker sits on a real quake.
    cell.members.sort((a, b) => data.impact[b] - data.impact[a]);
    const anchor = cell.members[0];
    clusters.push({
      cellKey,
      lon: data.lon[anchor],
      lat: data.lat[anchor],
      count: cell.count,
      severityRank: data.severityRank[anchor],
      memberIndices: cell.members,
    });
  }
  return { singleIndices, clusters };
}

/**
 * Filter a full grid to the singles/clusters whose representative point lies in
 * `view` (a margin-expanded camera rectangle). `null` renders everything (whole
 * globe / antimeridian-wrapping views, where clustering already bounds counts).
 */
export function filterGridToView(
  data: ClusterDataset,
  grid: GridResult,
  view: ViewRect | null,
): GridResult {
  if (!view) {
    return grid;
  }
  const inView = (lon: number, lat: number): boolean =>
    lon >= view.west && lon <= view.east && lat >= view.south && lat <= view.north;

  const singleIndices: number[] = [];
  for (const i of grid.singleIndices) {
    if (inView(data.lon[i], data.lat[i])) {
      singleIndices.push(i);
    }
  }
  const clusters: GridCluster[] = [];
  for (const cluster of grid.clusters) {
    if (inView(cluster.lon, cluster.lat)) {
      clusters.push(cluster);
    }
  }
  return { singleIndices, clusters };
}

// ── Worker message protocol ─────────────────────────────────────────────────

/** Main → worker: replace the dataset (buffers transferred). */
export interface ClusterDataMessage {
  type: "data";
  version: number;
  lon: ArrayBuffer;
  lat: ArrayBuffer;
  severityRank: ArrayBuffer;
  impact: ArrayBuffer;
  count: number;
}

/** Main → worker: request the visible singles/clusters for a level + view. */
export interface ClusterQueryMessage {
  type: "query";
  version: number;
  requestId: number;
  /** Distinguishes cache entries / LOD levels (uncluster has its own key). */
  levelKey: number;
  cellDeg: number;
  uncluster: boolean;
  view: ViewRect | null;
}

export type ClusterRequest = ClusterDataMessage | ClusterQueryMessage;

/** Worker → main: visible result for a query (singleIndices buffer transferred). */
export interface ClusterResultMessage {
  type: "result";
  version: number;
  requestId: number;
  levelKey: number;
  singleIndices: Uint32Array;
  clusters: GridCluster[];
}
