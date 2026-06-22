import { useEffect, useRef, useState } from "react";
import * as THREE from "three";
import { buildGeoJsonLineGeometry } from "@/components/globe/build-line-geometry";
import { GLOBE_RADIUS } from "@/components/globe/geo";

const COUNTRIES_GEOJSON_URL = "/geo/ne_110m_admin_0_countries.geojson";

/** Offline Natural Earth country borders rendered as line segments. */
export function GlobeCountryBorders() {
  const [geometry, setGeometry] = useState<THREE.BufferGeometry | null>(null);
  const geometryRef = useRef<THREE.BufferGeometry | null>(null);

  useEffect(() => {
    let cancelled = false;

    void fetch(COUNTRIES_GEOJSON_URL)
      .then((response) => {
        if (!response.ok) {
          throw new Error(`failed to load country borders (${response.status})`);
        }
        return response.json() as Promise<Parameters<typeof buildGeoJsonLineGeometry>[0]>;
      })
      .then((data) => {
        if (cancelled) {
          return;
        }
        const nextGeometry = buildGeoJsonLineGeometry(data, GLOBE_RADIUS * 1.004);
        geometryRef.current?.dispose();
        geometryRef.current = nextGeometry;
        setGeometry(nextGeometry);
      })
      .catch(() => {
        // Borders are optional visual polish; graticule still renders the map frame.
      });

    return () => {
      cancelled = true;
      geometryRef.current?.dispose();
      geometryRef.current = null;
    };
  }, []);

  if (!geometry) {
    return null;
  }

  return (
    <lineSegments geometry={geometry} renderOrder={3}>
      <lineBasicMaterial color="#cdf3ea" transparent opacity={0.5} depthWrite={false} toneMapped={false} />
    </lineSegments>
  );
}
