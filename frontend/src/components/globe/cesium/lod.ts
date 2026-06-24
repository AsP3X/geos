/**
 * Pure camera → on-screen-size math for the Cesium globe.
 *
 * Deliberately free of any `cesium` import so it can be unit-tested under a
 * plain Node environment (no WebGL / DOM). The marker-rendering modules
 * (`clusters.ts`, `quake-dots.ts`) re-export these so existing call sites keep
 * importing from their original locations.
 */

/** Finest grid step (degrees). Each coarser level doubles this so cell borders align. */
export const FINEST_CELL_DEG = 0.004;
export const COARSEST_CELL_DEG = 64;

/**
 * Dyadic LOD steps (coarse → fine). Aligned grids ensure zooming out only merges
 * cells — marker count never increases when the view coarsens.
 */
export const LEVEL_CELL_DEG: readonly number[] = (() => {
  const steps: number[] = [];
  let deg = FINEST_CELL_DEG;
  while (deg <= COARSEST_CELL_DEG + 1e-9) {
    steps.push(deg);
    deg *= 2;
  }
  return steps.reverse();
})();

/** Delay LOD flips until the view crosses the level boundary by this margin. */
export const LEVEL_HYSTERESIS_RATIO = 1.22;

/** Desired on-screen cell size (CSS px) used to pick the active LOD level. */
export const TARGET_CELL_PX = 26;

/**
 * When the view is zoomed in enough that a cluster cell would be smaller than
 * this (degrees), render every quake as its own dot instead of a merged bubble.
 * ~0.003° ≈ 330 m latitude — close enough to distinguish nearby events.
 */
export const UNCLUSTER_CELL_DEG = 0.003;

/** Sentinel `level` index returned when the uncluster path is active. */
export const UNCLUSTER_LEVEL = LEVEL_CELL_DEG.length;

/** Pick the precomputed level whose cell size best matches the target px size. */
export function pickLevelIndex(desiredDeg: number): number {
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
 * Like {@link pickLevelIndex} but resists rapid flips when the view hovers near
 * a level boundary (prevents clusters briefly splitting into extra singles).
 */
export function pickLevelIndexWithHysteresis(
  desiredDeg: number,
  currentLevel: number | null,
): number {
  const target = pickLevelIndex(desiredDeg);
  if (currentLevel === null || target === currentLevel || currentLevel === UNCLUSTER_LEVEL) {
    return target;
  }

  // Index 0 is coarsest; higher index = finer cells.
  if (target < currentLevel) {
    const boundary = Math.sqrt(LEVEL_CELL_DEG[target] * LEVEL_CELL_DEG[currentLevel]);
    if (desiredDeg < boundary * LEVEL_HYSTERESIS_RATIO) {
      return currentLevel;
    }
  } else {
    const boundary = Math.sqrt(LEVEL_CELL_DEG[currentLevel] * LEVEL_CELL_DEG[target]);
    if (desiredDeg > boundary / LEVEL_HYSTERESIS_RATIO) {
      return currentLevel;
    }
  }
  return target;
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

/**
 * Multiplier on the base quake-dot pixel size. Grows to 1.5× at continental
 * scale so sparse quakes stay visible, then eases back to 0.85× at full-globe
 * zoom so dense coastlines do not swamp the map.
 */
export function dotPixelScaleForHeight(cameraHeightMeters: number, maxZoomMeters: number): number {
  const close = 0.85;
  const regional = 1.5;
  const planet = 0.85;
  const nearHeight = 3.0e5;
  const regionalPeak = maxZoomMeters * 0.42;
  const planetBlendStart = maxZoomMeters * 0.7;

  if (cameraHeightMeters <= nearHeight) {
    return close;
  }
  if (cameraHeightMeters < regionalPeak) {
    const t = (cameraHeightMeters - nearHeight) / (regionalPeak - nearHeight);
    return close + (regional - close) * t;
  }
  if (cameraHeightMeters < planetBlendStart) {
    return regional;
  }
  if (cameraHeightMeters >= maxZoomMeters) {
    return planet;
  }
  const t = (cameraHeightMeters - planetBlendStart) / (maxZoomMeters - planetBlendStart);
  const smooth = t * t * (3 - 2 * t);
  return regional + (planet - regional) * smooth;
}
