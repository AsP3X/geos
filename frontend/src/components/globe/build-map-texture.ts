import * as THREE from "three";

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

export interface GeoJsonFeatureCollection {
  features: GeoJsonFeature[];
}

interface LandTextureOptions {
  width?: number;
  height?: number;
  oceanColor?: string;
  landColor?: string;
  anisotropy?: number;
}

/**
 * Paint land polygons onto an equirectangular canvas and return it as a texture.
 *
 * Filling 2D polygons avoids the facet/T-junction artifacts that geometry-based
 * land tessellation produces on a sphere. The projection (`x = (lon+180)/360`,
 * `y = (90-lat)/180`) matches both the sphere's default UVs and `latLonToVector3`,
 * so markers and borders stay aligned with the painted map.
 */
export function buildLandTexture(
  data: GeoJsonFeatureCollection,
  options: LandTextureOptions = {},
): THREE.CanvasTexture {
  const width = options.width ?? 4096;
  const height = options.height ?? 2048;
  const oceanColor = options.oceanColor ?? "#0b2742";
  const landColor = options.landColor ?? "#3c7d63";

  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;

  const ctx = canvas.getContext("2d");
  if (!ctx) {
    return new THREE.CanvasTexture(canvas);
  }

  ctx.fillStyle = oceanColor;
  ctx.fillRect(0, 0, width, height);

  ctx.fillStyle = landColor;
  for (const feature of data.features) {
    const geometry = feature.geometry;
    if (!geometry) {
      continue;
    }
    if (geometry.type === "Polygon") {
      fillPolygon(ctx, geometry.coordinates, width, height);
    } else if (geometry.type === "MultiPolygon") {
      for (const polygon of geometry.coordinates) {
        fillPolygon(ctx, polygon, width, height);
      }
    }
  }

  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.anisotropy = options.anisotropy ?? 1;
  texture.needsUpdate = true;
  return texture;
}

function fillPolygon(
  ctx: CanvasRenderingContext2D,
  rings: number[][][],
  width: number,
  height: number,
): void {
  // Draw at three horizontal offsets so polygons crossing the antimeridian
  // (after longitude unwrapping) wrap correctly instead of smearing.
  for (const offset of [-width, 0, width]) {
    ctx.beginPath();
    for (const ring of rings) {
      traceRing(ctx, ring, width, height, offset);
    }
    ctx.fill("evenodd");
  }
}

function traceRing(
  ctx: CanvasRenderingContext2D,
  ring: number[][],
  width: number,
  height: number,
  offsetX: number,
): void {
  if (ring.length < 2) {
    return;
  }

  let unwrappedLon = ring[0][0];
  let previousLon = ring[0][0];

  for (let index = 0; index < ring.length; index += 1) {
    const lon = ring[index][0];
    const lat = ring[index][1];

    if (index > 0) {
      let delta = lon - previousLon;
      if (delta > 180) {
        delta -= 360;
      } else if (delta < -180) {
        delta += 360;
      }
      unwrappedLon += delta;
    }
    previousLon = lon;

    const x = ((unwrappedLon + 180) / 360) * width + offsetX;
    const y = ((90 - lat) / 180) * height;

    if (index === 0) {
      ctx.moveTo(x, y);
    } else {
      ctx.lineTo(x, y);
    }
  }

  ctx.closePath();
}
