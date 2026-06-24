import { Color } from "cesium";
import type { Event } from "@/types/event";
import { severityToCesiumColor } from "@/components/globe/cesium/severity-colors";

export const QUAKE_DOT_PIXEL_SIZE = 12;
export const QUAKE_DOT_OUTLINE = Color.WHITE.withAlpha(0.88);
export const QUAKE_DOT_OUTLINE_WIDTH = 2;

const SELECTION_RING_OUTLINE_WIDTH = 2.5;

/**
 * Multiplier on {@link QUAKE_DOT_PIXEL_SIZE}. Grows to 1.5× at continental
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

/** Single quake markers — filled severity dot, no outline. */
export function quakePointStyle(
  severity: Event["severity"],
  scaleMultiplier: number = 1,
  pixelSize: number = QUAKE_DOT_PIXEL_SIZE,
) {
  return {
    color: severityToCesiumColor(severity),
    pixelSize: pixelSize * scaleMultiplier,
    outlineWidth: 0,
  };
}

/** Merged cluster marker — same fill, white ring distinguishes combined quakes. */
export function clusterDotStyle(severity: Event["severity"], scaleMultiplier: number = 1) {
  return {
    color: severityToCesiumColor(severity),
    pixelSize: QUAKE_DOT_PIXEL_SIZE * scaleMultiplier,
    outlineColor: QUAKE_DOT_OUTLINE,
    outlineWidth: QUAKE_DOT_OUTLINE_WIDTH,
  };
}

/** White selection ring drawn around a quake dot or cluster marker. */
export function selectionRingStyle(pixelSize: number) {
  return {
    color: Color.WHITE.withAlpha(0.0),
    pixelSize,
    outlineColor: Color.WHITE,
    outlineWidth: SELECTION_RING_OUTLINE_WIDTH,
  };
}
