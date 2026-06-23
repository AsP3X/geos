import { useEffect, useRef, useState } from "react";
import * as THREE from "three";
import { useFrame, useThree } from "@react-three/fiber";
import { buildGeoJsonLineGeometry } from "@/components/globe/build-line-geometry";

/** One level of detail: a GeoJSON source used while the camera is within range. */
export interface VectorLod {
  url: string;
  /** Use this level when camera distance ≤ this (scene units). Coarsest = Infinity. */
  maxDistance: number;
}

interface GlobeVectorLayerProps {
  /** Ordered coarse → fine (descending maxDistance). */
  lods: VectorLod[];
  radius: number;
  color: string;
  opacity: number;
  renderOrder?: number;
}

/**
 * Zoom-adaptive GeoJSON line overlay (coastlines / borders). Loads each level of
 * detail lazily the first time it's needed and caches it, swapping to the finest
 * source whose range the camera is within. Heavy 10m data is only fetched once
 * the user zooms in, keeping initial load light.
 */
export function GlobeVectorLayer({ lods, radius, color, opacity, renderOrder = 3 }: GlobeVectorLayerProps) {
  const camera = useThree((state) => state.camera);
  const cacheRef = useRef(new Map<string, THREE.BufferGeometry>());
  const loadingRef = useRef(new Set<string>());
  const activeUrlRef = useRef<string | null>(null);
  const [geometry, setGeometry] = useState<THREE.BufferGeometry | null>(null);

  useEffect(() => {
    const cache = cacheRef.current;
    return () => {
      for (const cached of cache.values()) {
        cached.dispose();
      }
      cache.clear();
    };
  }, []);

  const loadUrl = (url: string) => {
    const cache = cacheRef.current;
    if (cache.has(url) || loadingRef.current.has(url)) {
      return;
    }
    loadingRef.current.add(url);
    void fetch(url)
      .then((response) => (response.ok ? response.json() : Promise.reject(new Error(String(response.status)))))
      .then((data: Parameters<typeof buildGeoJsonLineGeometry>[0]) => {
        cache.set(url, buildGeoJsonLineGeometry(data, radius));
        loadingRef.current.delete(url);
      })
      .catch(() => {
        loadingRef.current.delete(url);
      });
  };

  useFrame(() => {
    const distance = camera.position.length();
    const cache = cacheRef.current;

    // Finest qualifying level (last wins), plus the coarsest qualifying one that
    // is already loaded as a fallback while a finer level streams in.
    let desired: string | null = null;
    let fallback: string | null = null;
    for (const lod of lods) {
      if (distance <= lod.maxDistance) {
        desired = lod.url;
        if (fallback === null && cache.has(lod.url)) {
          fallback = lod.url;
        }
      }
    }

    if (desired) {
      loadUrl(desired);
    }
    const best = desired && cache.has(desired) ? desired : fallback;
    if (best !== activeUrlRef.current) {
      activeUrlRef.current = best;
      setGeometry(best ? (cache.get(best) ?? null) : null);
    }
  });

  if (!geometry) {
    return null;
  }

  return (
    <lineSegments geometry={geometry} renderOrder={renderOrder}>
      <lineBasicMaterial color={color} transparent opacity={opacity} depthWrite={false} toneMapped={false} />
    </lineSegments>
  );
}
