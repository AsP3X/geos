import { useMemo, useRef } from "react";
import type * as THREE from "three";
import { Html } from "@react-three/drei";
import { useFrame, useThree } from "@react-three/fiber";
import type { Event } from "@/types/event";
import { latLonToVector3, MARKER_BASE_RADIUS, MARKER_SURFACE_RADIUS } from "@/components/globe/geo";
import { severityToColor } from "@/components/globe/severity-colors";
import { ScreenScaledInstances, type ScaledItem } from "@/components/globe/ScreenScaledInstances";

interface EventMarkersProps {
  /** Quakes rendered as individual spheres (cluster singletons). */
  events: Event[];
  /** World position of the selected quake, or null. Decoupled from `events`
   * so a selection inside a cluster still shows a ring. */
  selectedPosition: THREE.Vector3 | null;
  onSelect: (id: string) => void;
}

/** Fixed instance capacity; the draw count tracks the active singletons. */
const SINGLE_CAP = 20_000;
/** Screen-radius clamp (CSS px) for individual quake dots. */
const DOT_MIN_PX = 2.2;
const DOT_MAX_PX = 9;

/** Natural world radius from impact (0–100); higher impact reads larger. */
function impactWorld(impact: number): number {
  const scale = 0.6 + (Math.min(Math.max(impact, 0), 100) / 100) * 1.1;
  return MARKER_BASE_RADIUS * scale;
}

/**
 * Fixed-size screen-space ring around the selected dot. Rendered as a DOM
 * overlay (not a mesh on the globe surface) so it always faces the camera, keeps
 * a constant pixel size at any zoom, and draws on top — but it is hidden while
 * the selected quake is on the far (occluded) hemisphere.
 */
function SelectionRing({ position }: { position: THREE.Vector3 }) {
  const ringRef = useRef<HTMLDivElement>(null);
  const camera = useThree((state) => state.camera);
  const dir = useMemo(() => position.clone().normalize(), [position]);

  useFrame(() => {
    const el = ringRef.current;
    if (!el) {
      return;
    }
    const facing =
      dir.x * camera.position.x + dir.y * camera.position.y + dir.z * camera.position.z >
      MARKER_SURFACE_RADIUS;
    el.style.opacity = facing ? "1" : "0";
  });

  return (
    <Html position={position} center zIndexRange={[50, 30]} style={{ pointerEvents: "none" }}>
      <div
        ref={ringRef}
        style={{
          width: 26,
          height: 26,
          borderRadius: "50%",
          border: "2px solid rgba(255,255,255,0.95)",
          boxShadow: "0 0 6px rgba(255,255,255,0.65), inset 0 0 4px rgba(255,255,255,0.5)",
          transition: "opacity 120ms linear",
        }}
      />
    </Html>
  );
}

/** Instanced, screen-size-clamped sphere markers for individual quakes. */
export function EventMarkers({ events, selectedPosition, onSelect }: EventMarkersProps) {
  const items = useMemo<ScaledItem[]>(
    () =>
      events.slice(0, SINGLE_CAP).map((event) => ({
        id: event.id,
        position: latLonToVector3(event.location.lat, event.location.lon, MARKER_SURFACE_RADIUS),
        color: severityToColor(event.severity),
        baseWorld: impactWorld(event.impact_score),
      })),
    [events],
  );

  return (
    <>
      <ScreenScaledInstances
        items={items}
        capacity={SINGLE_CAP}
        segments={8}
        minPx={DOT_MIN_PX}
        maxPx={DOT_MAX_PX}
        renderOrder={2}
        onClickItem={(id) => onSelect(id)}
      />

      {selectedPosition ? <SelectionRing position={selectedPosition} /> : null}
    </>
  );
}
