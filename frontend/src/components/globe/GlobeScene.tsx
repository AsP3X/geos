import { useMemo } from "react";
import { Stars } from "@react-three/drei";
import { GlobeEarth } from "@/components/globe/GlobeEarth";
import { GlobeVectorLayer } from "@/components/globe/GlobeVectorLayer";
import { EventMarkers } from "@/components/globe/EventMarkers";
import { ClusterMarkers } from "@/components/globe/ClusterMarkers";
import { QuakeHeatLayer } from "@/components/globe/QuakeHeatLayer";
import { type GlobeLayers, isQuake } from "@/components/globe/layers";
import { GLOBE_RADIUS, latLonToVector3, MARKER_SURFACE_RADIUS } from "@/components/globe/geo";
import { useScreenClusters, type GlobeCluster } from "@/components/globe/useScreenClusters";
import { useDebouncedValue } from "@/hooks/useDebouncedValue";
import type { Event } from "@/types/event";

/** Country-border LOD: 110m far out, 50m mid, heavy 10m only when zoomed right in. */
const BORDER_LODS = [
  { url: "/geo/ne_110m_admin_0_countries.geojson", maxDistance: Infinity },
  { url: "/geo/ne_50m_admin_0_countries.geojson", maxDistance: GLOBE_RADIUS * 3 },
  { url: "/geo/ne_10m_admin_0_countries.geojson", maxDistance: GLOBE_RADIUS * 1.35 },
];

/** Coastline LOD: 50m vector once zoomed in (raster carries the wide view). The
 * heavy 10m coastline is intentionally omitted — borders supply the close detail
 * and rendering both 10m layers at once is too costly. */
const COASTLINE_LODS = [{ url: "/geo/ne_50m_land.geojson", maxDistance: GLOBE_RADIUS * 3 }];

interface GlobeSceneProps {
  events: Event[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onSelectCluster: (cluster: GlobeCluster) => void;
  layers: GlobeLayers;
  layerEpoch: number;
  /** True while additional map batches are still streaming in. */
  loadingMore?: boolean;
}

/** r3f scene graph: starfield, terminator lighting, earth, and event layers. */
export function GlobeScene({
  events,
  selectedId,
  onSelect,
  onSelectCluster,
  layers,
  layerEpoch,
  loadingMore = false,
}: GlobeSceneProps) {
  const quakeEvents = useMemo(() => events.filter(isQuake), [events]);
  // Throttle geometry rebuilds while batches stream in (point cloud renders the full set).
  const renderDebounceMs = quakeEvents.length > 2_500 || loadingMore ? 250 : 0;
  const debouncedQuakes = useDebouncedValue(quakeEvents, renderDebounceMs);

  // Dots split into singletons + clusters based on on-screen proximity.
  const { singles, clusters } = useScreenClusters(debouncedQuakes, layers.quakeDots);

  // Selection ring follows the chosen quake even when it lives inside a cluster.
  const selectedPosition = useMemo(() => {
    if (!selectedId) {
      return null;
    }
    const event = debouncedQuakes.find((item) => item.id === selectedId);
    return event
      ? latLonToVector3(event.location.lat, event.location.lon, MARKER_SURFACE_RADIUS)
      : null;
  }, [debouncedQuakes, selectedId]);

  return (
    <>
      <Stars
        radius={GLOBE_RADIUS * 30}
        depth={GLOBE_RADIUS * 20}
        count={2600}
        factor={3.4}
        saturation={0}
        fade
        speed={0.4}
      />

      {/* Low ambient keeps a defined day/night terminator for 3D form. */}
      <ambientLight intensity={0.18} />
      {/* Key light: warm sun grazing from upper-right creates the lit hemisphere. */}
      <directionalLight position={[5, 3, 4]} intensity={2.1} color="#fff2d6" />
      {/* Cool rim fill from behind for a subtle edge separation. */}
      <directionalLight position={[-4, -1, -5]} intensity={0.5} color="#3b6fb0" />

      <GlobeEarth />
      <GlobeVectorLayer
        lods={COASTLINE_LODS}
        radius={GLOBE_RADIUS * 1.0035}
        color="#bfe9ff"
        opacity={0.4}
        renderOrder={2}
      />
      <GlobeVectorLayer
        lods={BORDER_LODS}
        radius={GLOBE_RADIUS * 1.004}
        color="#cdf3ea"
        opacity={0.45}
        renderOrder={3}
      />
      {layers.quakeHeat ? (
        <QuakeHeatLayer events={debouncedQuakes} layerEpoch={layerEpoch} loadingMore={loadingMore} />
      ) : null}
      {layers.quakeDots ? (
        <>
          <EventMarkers
            events={singles}
            selectedPosition={selectedPosition}
            onSelect={onSelect}
          />
          <ClusterMarkers clusters={clusters} onSelectCluster={onSelectCluster} />
        </>
      ) : null}
    </>
  );
}
