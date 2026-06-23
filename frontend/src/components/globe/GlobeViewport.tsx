import { Suspense, useMemo } from "react";
import { Canvas } from "@react-three/fiber";
import { OrbitControls } from "@react-three/drei";
import type { Event } from "@/types/event";
import { GlobeScene } from "@/components/globe/GlobeScene";
import type { GlobeLayers } from "@/components/globe/layers";
import { isQuake, MARKER_DISPLAY_CAP } from "@/components/globe/layers";
import {
  CAMERA_DISTANCE,
  CAMERA_HEIGHT,
  ORBIT_MAX_DISTANCE,
  ORBIT_MIN_DISTANCE,
} from "@/components/globe/geo";

interface GlobeViewportProps {
  events: Event[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  layers: GlobeLayers;
  /** Total quakes matching the active filter (may exceed loaded points). */
  globeTotal?: number;
  /** True while waiting for the first map batch. */
  globeLoading?: boolean;
  /** True while additional map batches are streaming in. */
  globeLoadingMore?: boolean;
  /** Remount globe layers on filter change only. */
  globeEpoch?: number;
}

/** Full-bleed react-three-fiber globe with orbit controls and event markers. */
export function GlobeViewport({
  events,
  selectedId,
  onSelect,
  layers,
  globeTotal,
  globeLoading,
  globeLoadingMore,
  globeEpoch = 0,
}: GlobeViewportProps) {
  const quakeCount = events.filter(isQuake).length;
  const total = Math.max(globeTotal ?? 0, quakeCount);
  const dotsShown = Math.min(quakeCount, MARKER_DISPLAY_CAP);
  const stillLoading = Boolean(globeLoading && quakeCount === 0);
  const loadingMore = Boolean(globeLoadingMore);
  const heatTruncated = total > quakeCount;
  const dotsSampled = quakeCount > MARKER_DISPLAY_CAP;

  const statusLabel = useMemo(() => {
    if (stillLoading) {
      return total > 0
        ? `Loading quakes… ${quakeCount.toLocaleString()} / ${total.toLocaleString()}`
        : "Loading quakes…";
    }
    if (quakeCount === 0) {
      return "No quakes match filters";
    }
    let label: string;
    if (loadingMore || heatTruncated) {
      label = `${quakeCount.toLocaleString()} / ${total.toLocaleString()} quakes loaded`;
      if (loadingMore) {
        label += "…";
      }
    } else {
      label = `${total.toLocaleString()} quake${total === 1 ? "" : "s"}`;
    }
    if (dotsSampled && layers.quakeDots) {
      label += ` · ${dotsShown.toLocaleString()} dots`;
    }
    return label;
  }, [
    stillLoading,
    total,
    quakeCount,
    loadingMore,
    heatTruncated,
    dotsSampled,
    layers.quakeDots,
    dotsShown,
  ]);
  return (
    <div className="relative size-full min-h-full overflow-hidden bg-[#05070d]">
      <Canvas
        camera={{ position: [0, CAMERA_HEIGHT, CAMERA_DISTANCE], fov: 42, near: 0.1, far: 200 }}
        dpr={[1, 2]}
        gl={{ antialias: true, alpha: false }}
      >
        <color attach="background" args={["#05070d"]} />
        <Suspense fallback={null}>
          <GlobeScene
            events={events}
            selectedId={selectedId}
            onSelect={onSelect}
            layers={layers}
            layerEpoch={globeEpoch}
            loadingMore={globeLoadingMore}
          />
        </Suspense>
        <OrbitControls
          enablePan={false}
          enableDamping
          dampingFactor={0.08}
          autoRotate
          autoRotateSpeed={0.32}
          minDistance={ORBIT_MIN_DISTANCE}
          maxDistance={ORBIT_MAX_DISTANCE}
          rotateSpeed={0.45}
          zoomSpeed={0.7}
        />
      </Canvas>

      {/* Cinematic vignette to focus the eye on the globe. */}
      <div className="pointer-events-none absolute inset-0 [background:radial-gradient(circle_at_50%_45%,transparent_42%,rgba(5,7,13,0.55)_100%)]" />

      <div className="glass-panel pointer-events-none absolute bottom-4 left-4 rounded-full px-3 py-1.5 text-xs text-foreground/55">
        {statusLabel}
      </div>
    </div>
  );
}
