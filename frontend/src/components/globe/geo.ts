import * as THREE from "three";

/** Radius of the globe mesh in scene units. */
export const GLOBE_RADIUS = 2;

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
