import { Cartesian3, Color, NearFarScalar } from "cesium";
import type { Event } from "@/types/event";
import { severityToCesiumColor } from "@/components/globe/cesium/severity-colors";

/** Matches `MAX_ZOOM_METERS` in `CesiumGlobeViewport` for distance scaling. */
const GLOBE_MAX_ZOOM_METERS = 4.5e7;

export const QUAKE_DOT_PIXEL_SIZE = 12;
export const CLUSTER_DOT_PIXEL_SIZE = 14;
export const QUAKE_DOT_OUTLINE = Color.WHITE.withAlpha(0.88);
export const QUAKE_DOT_OUTLINE_WIDTH = 2;
export const QUAKE_DOT_SCALE_BY_DISTANCE = new NearFarScalar(
  3.0e5,
  0.85,
  GLOBE_MAX_ZOOM_METERS,
  1.5,
);

const SELECTION_RING_OUTLINE_WIDTH = 2.5;

/** Shared Cesium `PointPrimitive` styling for single quake markers. */
export function quakePointStyle(
  severity: Event["severity"],
  pixelSize: number = QUAKE_DOT_PIXEL_SIZE,
) {
  return {
    color: severityToCesiumColor(severity),
    pixelSize,
    outlineColor: QUAKE_DOT_OUTLINE,
    outlineWidth: QUAKE_DOT_OUTLINE_WIDTH,
    scaleByDistance: QUAKE_DOT_SCALE_BY_DISTANCE,
  };
}

/**
 * Three-dot triangle glyph (eye-space offsets) for merged clusters. Reads as
 * "multiple events here" without a numeric label and stays legible at any zoom.
 */
export function clusterGlyphStyles(severity: Event["severity"]) {
  const withOffset = (pixelSize: number, eyeOffset: Cartesian3) => ({
    ...quakePointStyle(severity, pixelSize),
    eyeOffset,
  });

  return [
    withOffset(CLUSTER_DOT_PIXEL_SIZE, new Cartesian3(0, 5, 0)),
    withOffset(QUAKE_DOT_PIXEL_SIZE - 1, new Cartesian3(-9, -6, 0)),
    withOffset(QUAKE_DOT_PIXEL_SIZE - 1, new Cartesian3(9, -6, 0)),
  ];
}

/** White selection ring drawn around a quake dot or cluster marker. */
export function selectionRingStyle(pixelSize: number) {
  return {
    color: Color.WHITE.withAlpha(0.0),
    pixelSize,
    outlineColor: Color.WHITE,
    outlineWidth: SELECTION_RING_OUTLINE_WIDTH,
    scaleByDistance: QUAKE_DOT_SCALE_BY_DISTANCE,
  };
}
