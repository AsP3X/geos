import { useEffect, useRef, useState } from "react";
import * as THREE from "three";
import { useFrame, useThree } from "@react-three/fiber";
import type { Event } from "@/types/event";
import { MARKER_SURFACE_RADIUS } from "@/components/globe/geo";
import { severityToColor } from "@/components/globe/severity-colors";

const DEG2RAD = Math.PI / 180;

/**
 * Pre-binned cluster resolutions (cell size in degrees, coarse → fine). The
 * merged spheres for each level are computed once and cached; zooming just
 * selects the level whose cells project near {@link TARGET_CELL_PX} on screen.
 */
const LEVEL_CELL_DEG = [
  60, 40, 26, 17, 11, 7, 4.5, 3, 2, 1.3, 0.85, 0.55, 0.36, 0.24, 0.16, 0.1, 0.06, 0.035,
];
/** Desired on-screen cell size (CSS px) used to pick the active LOD level. */
const TARGET_CELL_PX = 26;
/**
 * How long the camera must hold still before the horizon cull is re-evaluated.
 * Rotation keeps the same LOD level, so spheres just orbit without a recompute;
 * deferring the cull until the camera *settles* avoids strobing mid-gesture. A
 * change in LOD level (i.e. zooming) bypasses this and applies immediately.
 */
const SETTLE_MS = 110;
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
  position: THREE.Vector3;
  count: number;
  /** Member quakes (highest-impact first), capped at {@link MEMBER_CAP}. */
  members: Event[];
  color: THREE.Color;
}

export interface ScreenClusters {
  /** Cells holding exactly one quake — rendered as ordinary dots. */
  singles: Event[];
  /** Cells holding 2+ quakes — rendered as labeled bubbles. */
  clusters: GlobeCluster[];
}

/** Precomputed single dot with its unit surface direction (for horizon culling). */
interface SingleEntry {
  event: Event;
  dx: number;
  dy: number;
  dz: number;
}

/** Precomputed cluster bubble with its unit surface direction. */
interface ClusterEntry {
  cluster: GlobeCluster;
  dx: number;
  dy: number;
  dz: number;
}

/** All merged spheres for one LOD level, computed once and reused across frames. */
interface LevelData {
  singles: SingleEntry[];
  clusters: ClusterEntry[];
}

interface Cell {
  count: number;
  sx: number;
  sy: number;
  sz: number;
  first: Event;
  members: Event[];
  topSeverity: number;
}

const EMPTY: ScreenClusters = { singles: [], clusters: [] };

/**
 * Bin every event into an (approximately equal-area) lat/lon grid of the given
 * cell size and materialize the merged spheres. Longitude cell width is widened
 * toward the poles by 1/cos(lat) so cells stay roughly square. Pure function of
 * the event set + cell size, so results are cacheable per level.
 */
function buildLevel(events: Event[], cellDeg: number): LevelData {
  const cells = new Map<string, Cell>();

  for (let i = 0; i < events.length; i += 1) {
    const event = events[i];
    const lat = event.location.lat;
    const lon = event.location.lon;
    const cosLat = Math.max(Math.cos(lat * DEG2RAD), 0.15);
    const lonCellDeg = cellDeg / cosLat;
    const cellX = Math.floor((lon + 180) / lonCellDeg);
    const cellY = Math.floor((lat + 90) / cellDeg);
    const key = `${cellY}:${cellX}`;

    const phi = (90 - lat) * DEG2RAD;
    const theta = (lon + 180) * DEG2RAD;
    const sinPhi = Math.sin(phi);
    const ux = -sinPhi * Math.cos(theta);
    const uy = Math.cos(phi);
    const uz = sinPhi * Math.sin(theta);

    const rank = SEVERITY_RANK[event.severity];

    const existing = cells.get(key);
    if (existing) {
      existing.count += 1;
      existing.sx += ux;
      existing.sy += uy;
      existing.sz += uz;
      if (existing.members.length < MEMBER_CAP) {
        existing.members.push(event);
      }
      if (rank > existing.topSeverity) {
        existing.topSeverity = rank;
      }
    } else {
      cells.set(key, { count: 1, sx: ux, sy: uy, sz: uz, first: event, members: [event], topSeverity: rank });
    }
  }

  const singles: SingleEntry[] = [];
  const clusters: ClusterEntry[] = [];
  for (const cell of cells.values()) {
    const len = Math.hypot(cell.sx, cell.sy, cell.sz) || 1;
    const dx = cell.sx / len;
    const dy = cell.sy / len;
    const dz = cell.sz / len;

    if (cell.count === 1) {
      singles.push({ event: cell.first, dx, dy, dz });
      continue;
    }

    const members = cell.members.slice().sort((a, b) => b.impact_score - a.impact_score);
    const topSeverity = members[0]?.severity ?? "info";
    const position = new THREE.Vector3(dx, dy, dz).multiplyScalar(MARKER_SURFACE_RADIUS);
    clusters.push({
      cluster: {
        id: `cl_${members[0]?.id ?? `${cell.count}`}_${cell.count}`,
        position,
        count: cell.count,
        members,
        color: severityToColor(topSeverity).clone(),
      },
      dx,
      dy,
      dz,
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
 * Level-of-detail marker clustering for the globe. The merged spheres for each
 * zoom level are **precomputed once and cached** (see {@link buildLevel}); each
 * settle just selects the level matching the current zoom and applies a cheap
 * horizon cull, so merging tightens when zoomed out and dissolves into dots when
 * zoomed in — with no per-frame re-binning and no strobing mid-gesture.
 */
export function useScreenClusters(events: Event[], enabled: boolean): ScreenClusters {
  const camera = useThree((state) => state.camera);
  const size = useThree((state) => state.size);

  const [result, setResult] = useState<ScreenClusters>(EMPTY);
  const eventsRef = useRef(events);
  const cacheRef = useRef(new Map<number, LevelData>());
  const lastSeenMatrixRef = useRef(new THREE.Matrix4());
  const lastMoveTimeRef = useRef(0);
  const lastLevelRef = useRef(-1);
  const pendingRef = useRef(true);
  const dirtyRef = useRef(true);

  // New event set invalidates every precomputed level.
  useEffect(() => {
    eventsRef.current = events;
    cacheRef.current.clear();
    dirtyRef.current = true;
  }, [events]);

  // Viewport / toggle changes only require reselecting the level.
  useEffect(() => {
    dirtyRef.current = true;
  }, [size.height, enabled]);

  useFrame(() => {
    if (!enabled) {
      return;
    }
    const now = performance.now();

    const moved = !camera.matrixWorld.equals(lastSeenMatrixRef.current);
    if (moved) {
      lastSeenMatrixRef.current.copy(camera.matrixWorld);
      lastMoveTimeRef.current = now;
      pendingRef.current = true;
    }

    // Cheap LOD pick for the current zoom (cell size ≈ TARGET_CELL_PX on screen).
    const perspective = camera as THREE.PerspectiveCamera;
    const pxPerUnit = size.height / (2 * Math.tan((perspective.fov * DEG2RAD) / 2));
    const distance = camera.position.length();
    const arcWorld = (TARGET_CELL_PX * distance) / pxPerUnit;
    const desiredDeg = arcWorld / MARKER_SURFACE_RADIUS / DEG2RAD;
    const levelIndex = pickLevelIndex(desiredDeg);
    const levelChanged = levelIndex !== lastLevelRef.current;

    if (!pendingRef.current && !dirtyRef.current) {
      return;
    }
    // Zooming (LOD level change) reveals/merges dots immediately; rotation at
    // the same level waits for the camera to settle to avoid strobing.
    if (now - lastMoveTimeRef.current < SETTLE_MS && !levelChanged) {
      return;
    }
    pendingRef.current = false;
    dirtyRef.current = false;
    lastLevelRef.current = levelIndex;

    let level = cacheRef.current.get(levelIndex);
    if (!level) {
      level = buildLevel(eventsRef.current, LEVEL_CELL_DEG[levelIndex]);
      cacheRef.current.set(levelIndex, level);
    }

    // Horizon cull: a surface point faces the camera when dir·cam > surface R.
    const camPos = camera.position;
    const singles: Event[] = [];
    const clusters: GlobeCluster[] = [];
    for (const entry of level.singles) {
      if (entry.dx * camPos.x + entry.dy * camPos.y + entry.dz * camPos.z > MARKER_SURFACE_RADIUS) {
        singles.push(entry.event);
      }
    }
    for (const entry of level.clusters) {
      if (entry.dx * camPos.x + entry.dy * camPos.y + entry.dz * camPos.z > MARKER_SURFACE_RADIUS) {
        clusters.push(entry.cluster);
      }
    }

    setResult({ singles, clusters });
  });

  return enabled ? result : EMPTY;
}
