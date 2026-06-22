import * as THREE from "three";
import { latLonToVector3 } from "@/components/globe/geo";

interface GeoJsonPolygon {
  type: "Polygon";
  coordinates: number[][][];
}

interface GeoJsonMultiPolygon {
  type: "MultiPolygon";
  coordinates: number[][][][];
}

type GeoJsonGeometry = GeoJsonPolygon | GeoJsonMultiPolygon;

interface GeoJsonFeature {
  geometry: GeoJsonGeometry | null;
}

interface GeoJsonFeatureCollection {
  features: GeoJsonFeature[];
}

/** Build merged line segments for GeoJSON polygon/multipolygon rings on the globe. */
export function buildGeoJsonLineGeometry(
  data: GeoJsonFeatureCollection,
  radius: number,
): THREE.BufferGeometry {
  const points: THREE.Vector3[] = [];

  for (const feature of data.features) {
    const geometry = feature.geometry;
    if (!geometry) {
      continue;
    }

    if (geometry.type === "Polygon") {
      appendPolygonRings(geometry.coordinates, radius, points);
    } else if (geometry.type === "MultiPolygon") {
      for (const polygon of geometry.coordinates) {
        appendPolygonRings(polygon, radius, points);
      }
    }
  }

  return new THREE.BufferGeometry().setFromPoints(points);
}

function appendPolygonRings(
  rings: number[][][],
  radius: number,
  points: THREE.Vector3[],
): void {
  for (const ring of rings) {
    appendRing(ring, radius, points);
  }
}

function appendRing(ring: number[][], radius: number, points: THREE.Vector3[]): void {
  if (ring.length < 2) {
    return;
  }

  for (let index = 0; index < ring.length - 1; index += 1) {
    const [lonA, latA] = ring[index];
    const [lonB, latB] = ring[index + 1];

    // Skip segments that jump across the antimeridian (would wrap incorrectly).
    if (Math.abs(lonA - lonB) > 180) {
      continue;
    }

    points.push(latLonToVector3(latA, lonA, radius));
    points.push(latLonToVector3(latB, lonB, radius));
  }
}

/** Build latitude/longitude graticule lines on the globe surface. */
export function buildGraticuleGeometry(
  radius: number,
  stepDegrees = 30,
): THREE.BufferGeometry {
  const points: THREE.Vector3[] = [];

  for (let lat = -90 + stepDegrees; lat < 90; lat += stepDegrees) {
    appendParallel(lat, radius, stepDegrees, points);
  }

  for (let lon = -180; lon < 180; lon += stepDegrees) {
    appendMeridian(lon, radius, stepDegrees, points);
  }

  return new THREE.BufferGeometry().setFromPoints(points);
}

function appendParallel(
  lat: number,
  radius: number,
  stepDegrees: number,
  points: THREE.Vector3[],
): void {
  for (let lon = -180; lon < 180; lon += stepDegrees) {
    points.push(latLonToVector3(lat, lon, radius));
    points.push(latLonToVector3(lat, lon + stepDegrees, radius));
  }
}

function appendMeridian(
  lon: number,
  radius: number,
  stepDegrees: number,
  points: THREE.Vector3[],
): void {
  for (let lat = -90; lat < 90; lat += stepDegrees) {
    points.push(latLonToVector3(lat, lon, radius));
    points.push(latLonToVector3(lat + stepDegrees, lon, radius));
  }
}
