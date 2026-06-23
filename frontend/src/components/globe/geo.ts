import * as THREE from "three";

/** Radius of the globe mesh in scene units. */
export const GLOBE_RADIUS = 3;

/**
 * Scene values derived from {@link GLOBE_RADIUS} so the camera framing, zoom
 * limits, and marker proportions stay identical regardless of globe size.
 * Keep these as ratios (not hardcoded units) so resizing the globe never
 * desyncs the camera, controls, or overlays.
 */

/** Default camera offset above the equator (slight downward tilt). */
export const CAMERA_HEIGHT = GLOBE_RADIUS * 0.2;
/** Default camera distance from globe center; lower ratio = larger on screen. */
export const CAMERA_DISTANCE = GLOBE_RADIUS * 2.35;
/** Closest the camera may orbit (just above the surface). */
export const ORBIT_MIN_DISTANCE = GLOBE_RADIUS * 1.55;
/** Farthest the camera may orbit out. */
export const ORBIT_MAX_DISTANCE = GLOBE_RADIUS * 6;
/** Base radius of an event marker dot relative to the globe. */
export const MARKER_BASE_RADIUS = GLOBE_RADIUS * 0.0085;

/**
 * Convert WGS84 lat/lon to a unit direction on the globe surface.
 * Y is up (north pole); +X is the prime meridian at the equator.
 */
export function latLonToVector3(
  lat: number,
  lon: number,
  radius: number = GLOBE_RADIUS,
): THREE.Vector3 {
  const phi = THREE.MathUtils.degToRad(90 - lat);
  const theta = THREE.MathUtils.degToRad(lon + 180);

  return new THREE.Vector3(
    -radius * Math.sin(phi) * Math.cos(theta),
    radius * Math.cos(phi),
    radius * Math.sin(phi) * Math.sin(theta),
  );
}
