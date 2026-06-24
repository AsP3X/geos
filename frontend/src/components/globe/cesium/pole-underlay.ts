import { apiBaseUrl } from "@/lib/env";

/** Equirectangular pole-fill resolution. */
const TEX_W = 2048;
const TEX_H = 1024;
const TILE_SIZE = 256;

/** Latitude where we start blending Sentinel edge colors over the base texture. */
const BLEND_START_DEG = 78;
const MERCATOR_LIMIT_DEG = 85.05112878;

const DAYMAP_URL = "/textures/earth_daymap.jpg";

/** Hidden mid-latitude fill when the daymap asset is unavailable. */
const HIDDEN_OCEAN: Rgb = { r: 30, g: 60, b: 90 };

/** Fallback edge tones when Sentinel tiles are unavailable (s2cloudless-like). */
const FALLBACK_NORTH_EDGE: Rgb = { r: 38, g: 72, b: 108 };
const FALLBACK_SOUTH_EDGE: Rgb = { r: 42, g: 78, b: 112 };

interface Rgb {
  r: number;
  g: number;
  b: number;
}

export interface PolarEdgeSamples {
  north: Uint8ClampedArray;
  south: Uint8ClampedArray;
  width: number;
}

function smoothstep(edge0: number, edge1: number, x: number): number {
  const t = Math.min(Math.max((x - edge0) / (edge1 - edge0), 0), 1);
  return t * t * (3 - 2 * t);
}

function lerpRgb(a: Rgb, b: Rgb, t: number): Rgb {
  return {
    r: a.r * (1 - t) + b.r * t,
    g: a.g * (1 - t) + b.g * t,
    b: a.b * (1 - t) + b.b * t,
  };
}

function loadImage(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("image load failed"));
    img.src = url;
  });
}

function sampleEdgeRgb(edge: Uint8ClampedArray, width: number, lon: number): Rgb {
  const u = ((lon + 180) / 360) * (width - 1);
  const i0 = Math.floor(u);
  const frac = u - i0;
  const i1 = Math.min(i0 + 1, width - 1);
  const o0 = i0 * 4;
  const o1 = i1 * 4;
  return {
    r: edge[o0] * (1 - frac) + edge[o1] * frac,
    g: edge[o0 + 1] * (1 - frac) + edge[o1 + 1] * frac,
    b: edge[o0 + 2] * (1 - frac) + edge[o1 + 2] * frac,
  };
}

function extrapolateTowardPole(base: Rgb, lat: number): Rgb {
  const t = Math.min((Math.abs(lat) - MERCATOR_LIMIT_DEG) / (90 - MERCATOR_LIMIT_DEG), 1);
  const gray = (base.r + base.g + base.b) / 3;
  const mix = t * 0.28;
  return {
    r: base.r * (1 - mix) + gray * 0.92 * mix + 12 * t,
    g: base.g * (1 - mix) + gray * 0.96 * mix + 16 * t,
    b: base.b * (1 - mix) + gray * 1.02 * mix + 18 * t,
  };
}

function polarRgb(
  edges: PolarEdgeSamples | null,
  lon: number,
  lat: number,
  isNorth: boolean,
): Rgb {
  const absLat = Math.abs(lat);
  let edgeRgb: Rgb;
  if (edges) {
    const edge = isNorth ? edges.north : edges.south;
    edgeRgb = sampleEdgeRgb(edge, edges.width, lon);
  } else {
    edgeRgb = isNorth ? FALLBACK_NORTH_EDGE : FALLBACK_SOUTH_EDGE;
  }
  return absLat > MERCATOR_LIMIT_DEG ? extrapolateTowardPole(edgeRgb, lat) : edgeRgb;
}

function sampleDaymap(daymap: ImageData, lon: number, lat: number): Rgb {
  const px = Math.min(
    TEX_W - 1,
    Math.max(0, Math.floor(((lon + 180) / 360) * TEX_W)),
  );
  const py = Math.min(
    TEX_H - 1,
    Math.max(0, Math.floor(((90 - lat) / 180) * TEX_H)),
  );
  const o = (py * TEX_W + px) * 4;
  return { r: daymap.data[o], g: daymap.data[o + 1], b: daymap.data[o + 2] };
}

async function loadDaymapPixels(): Promise<ImageData | null> {
  try {
    const img = await loadImage(DAYMAP_URL);
    const scratch = document.createElement("canvas");
    scratch.width = TEX_W;
    scratch.height = TEX_H;
    const ctx = scratch.getContext("2d");
    if (!ctx) {
      return null;
    }
    ctx.drawImage(img, 0, 0, TEX_W, TEX_H);
    return ctx.getImageData(0, 0, TEX_W, TEX_H);
  } catch {
    return null;
  }
}

/**
 * Read one scanline from the northern- or southern-most Sentinel row at `zoom`
 * so the pole fill can match the Mercator imagery at ±~85°.
 *
 * `tilesToken` is the signed token from `GET /api/v1/tiles/session`, not the
 * user's access JWT.
 */
export async function sampleSentinelPolarEdges(
  tilesToken: string,
  zoom = 3,
): Promise<PolarEdgeSamples | null> {
  const tiles = 1 << zoom;
  const width = tiles * TILE_SIZE;
  const north = new Uint8ClampedArray(width * 4);
  const south = new Uint8ClampedArray(width * 4);
  const base = apiBaseUrl();

  const readRow = async (x: number, y: number, row: number, dest: Uint8ClampedArray) => {
    const url = `${base}/api/v1/tiles/sentinel2/${zoom}/${x}/${y}.jpg?token=${encodeURIComponent(tilesToken)}`;
    const img = await loadImage(url);
    const scratch = document.createElement("canvas");
    scratch.width = TILE_SIZE;
    scratch.height = TILE_SIZE;
    const ctx = scratch.getContext("2d");
    if (!ctx) {
      throw new Error("canvas unsupported");
    }
    ctx.drawImage(img, 0, 0);
    const line = ctx.getImageData(0, row, TILE_SIZE, 1).data;
    dest.set(line, x * TILE_SIZE * 4);
  };

  try {
    await Promise.all(
      Array.from({ length: tiles }, (_, x) =>
        Promise.all([
          readRow(x, 0, 0, north),
          readRow(x, tiles - 1, TILE_SIZE - 1, south),
        ]),
      ),
    );
    return { north, south, width };
  } catch {
    return null;
  }
}

/**
 * Build a fully opaque equirectangular underlay: Blue Marble base (hidden under
 * Mercator tiles at mid-latitudes) with Sentinel edge colors blended over the
 * polar caps so the fill matches the main basemap.
 */
export function buildPoleUnderlayCanvas(
  edges: PolarEdgeSamples | null,
  daymap: ImageData | null,
): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = TEX_W;
  canvas.height = TEX_H;
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    return canvas;
  }

  const image = ctx.createImageData(TEX_W, TEX_H);

  for (let py = 0; py < TEX_H; py += 1) {
    const lat = 90 - (py / TEX_H) * 180;
    const absLat = Math.abs(lat);
    const isNorth = lat >= 0;

    for (let px = 0; px < TEX_W; px += 1) {
      const lon = (px / TEX_W) * 360 - 180;
      const base = daymap ? sampleDaymap(daymap, lon, lat) : HIDDEN_OCEAN;

      let rgb = base;
      if (absLat >= BLEND_START_DEG) {
        const polar = polarRgb(edges, lon, lat, isNorth);
        const blend = smoothstep(BLEND_START_DEG, 88, absLat);
        rgb = lerpRgb(base, polar, blend);
      }

      const o = (py * TEX_W + px) * 4;
      image.data[o] = Math.round(rgb.r);
      image.data[o + 1] = Math.round(rgb.g);
      image.data[o + 2] = Math.round(rgb.b);
      image.data[o + 3] = 255;
    }
  }

  ctx.putImageData(image, 0, 0);
  return canvas;
}

/** Sample Sentinel edges when possible, then build the pole-fill canvas. */
export async function createPoleUnderlayCanvas(tilesToken?: string): Promise<HTMLCanvasElement> {
  const [edges, daymap] = await Promise.all([
    tilesToken ? sampleSentinelPolarEdges(tilesToken) : Promise.resolve(null),
    loadDaymapPixels(),
  ]);
  return buildPoleUnderlayCanvas(edges, daymap);
}

export { DAYMAP_URL };
