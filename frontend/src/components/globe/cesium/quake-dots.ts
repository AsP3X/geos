import { Color } from "cesium";
import type { Event } from "@/types/event";
import { severityToCesiumColor } from "@/components/globe/cesium/severity-colors";

// Re-exported for existing call sites that import it from this module.
export { dotPixelScaleForHeight } from "@/components/globe/cesium/lod";

export const QUAKE_DOT_PIXEL_SIZE = 12;
export const QUAKE_DOT_OUTLINE = Color.WHITE.withAlpha(0.88);
export const QUAKE_DOT_OUTLINE_WIDTH = 2;

const SELECTION_RING_OUTLINE_WIDTH = 2.5;

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
