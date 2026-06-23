import { useEffect, useMemo } from "react";
import * as THREE from "three";
import type { Event } from "@/types/event";
import { GLOBE_RADIUS } from "@/components/globe/geo";
import { quakeStrength } from "@/components/globe/layers";
import { useDebouncedValue } from "@/hooks/useDebouncedValue";

/** Equirectangular heat canvas resolution (smooth enough, cheap to repaint). */
const TEX_W = 1024;
const TEX_H = 512;
/** Thin shell above the surface so the field sits on the globe, not floating. */
const HEAT_RADIUS = GLOBE_RADIUS * 1.0035;

/**
 * Thermal colormap stops `[t, r, g, b]` (cool → hot). Painting a continuous
 * field through this ramp reads as a real heatmap, unlike additively blended
 * sprites which wash out and float above the surface.
 */
const RAMP: ReadonlyArray<readonly [number, number, number, number]> = [
  [0.0, 38, 70, 160],
  [0.28, 20, 150, 220],
  [0.48, 22, 180, 120],
  [0.66, 250, 205, 40],
  [0.83, 248, 130, 35],
  [1.0, 235, 45, 45],
];

function rampColor(t: number): [number, number, number] {
  const clamped = Math.min(Math.max(t, 0), 1);
  for (let i = 1; i < RAMP.length; i += 1) {
    const [t1, r1, g1, b1] = RAMP[i];
    if (clamped <= t1) {
      const [t0, r0, g0, b0] = RAMP[i - 1];
      const f = (clamped - t0) / (t1 - t0 || 1);
      return [r0 + (r1 - r0) * f, g0 + (g1 - g0) * f, b0 + (b1 - b0) * f];
    }
  }
  const last = RAMP[RAMP.length - 1];
  return [last[1], last[2], last[3]];
}

/**
 * Accumulate a gaussian splat per quake into an equirectangular intensity
 * field, then map intensity through the thermal ramp. Density (overlapping
 * splats) and magnitude both push regions toward hot.
 */
function buildHeatTexture(events: Event[]): THREE.CanvasTexture | null {
  if (events.length === 0) {
    return null;
  }

  const intensity = new Float32Array(TEX_W * TEX_H);

  for (const event of events) {
    const strength = quakeStrength(event);
    const cx = ((event.location.lon + 180) / 360) * TEX_W;
    const cy = ((90 - event.location.lat) / 180) * TEX_H;
    const weight = 0.35 + strength * 0.95;

    // Radius in pixels grows with strength; stretch horizontally toward the
    // poles so the splat stays roughly circular on the sphere.
    const ry = 9 + strength * 24;
    const latRad = (event.location.lat * Math.PI) / 180;
    const xStretch = Math.min(1 / Math.max(Math.cos(latRad), 0.16), 6);
    const rx = ry * xStretch;

    const yStart = Math.max(0, Math.floor(cy - ry));
    const yEnd = Math.min(TEX_H - 1, Math.ceil(cy + ry));
    const xStart = Math.floor(cx - rx);
    const xEnd = Math.ceil(cx + rx);

    for (let y = yStart; y <= yEnd; y += 1) {
      const dy = (y - cy) / ry;
      const row = y * TEX_W;
      for (let x = xStart; x <= xEnd; x += 1) {
        const dx = (x - cx) / rx;
        const d2 = dx * dx + dy * dy;
        if (d2 > 1) {
          continue;
        }
        // Wrap longitude across the antimeridian.
        const xi = ((x % TEX_W) + TEX_W) % TEX_W;
        intensity[row + xi] += weight * Math.exp(-d2 * 3.0);
      }
    }
  }

  const canvas = document.createElement("canvas");
  canvas.width = TEX_W;
  canvas.height = TEX_H;
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    return null;
  }

  const image = ctx.createImageData(TEX_W, TEX_H);
  for (let i = 0; i < intensity.length; i += 1) {
    // Saturating normalization: lone quakes stay cool, clusters drive hot.
    const t = 1 - Math.exp(-1.15 * intensity[i]);
    const o = i * 4;
    if (t < 0.02) {
      image.data[o + 3] = 0;
      continue;
    }
    const [r, g, b] = rampColor(t);
    image.data[o] = r;
    image.data[o + 1] = g;
    image.data[o + 2] = b;
    image.data[o + 3] = Math.min(t * 1.25, 0.82) * 255;
  }
  ctx.putImageData(image, 0, 0);

  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.needsUpdate = true;
  return texture;
}

interface QuakeHeatLayerProps {
  /** Pre-filtered quake events. */
  events: Event[];
  layerEpoch: number;
  loadingMore?: boolean;
}

/** Texture-painted thermal field mapped onto a thin shell over the globe. */
export function QuakeHeatLayer({ events, layerEpoch, loadingMore = false }: QuakeHeatLayerProps) {
  const buildDebounceMs = events.length > 2_500 || loadingMore ? 400 : 0;
  const buildEvents = useDebouncedValue(events, buildDebounceMs);
  const texture = useMemo(() => buildHeatTexture(buildEvents), [buildEvents]);

  useEffect(() => () => texture?.dispose(), [texture]);

  if (!texture) {
    return null;
  }

  return (
    <mesh key={layerEpoch} renderOrder={1}>
      <sphereGeometry args={[HEAT_RADIUS, 96, 96]} />
      <meshBasicMaterial
        map={texture}
        transparent
        depthWrite={false}
        side={THREE.FrontSide}
        toneMapped={false}
      />
    </mesh>
  );
}
