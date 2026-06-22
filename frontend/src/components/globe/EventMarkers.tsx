import { useCallback, useMemo } from "react";
import { Instance, Instances } from "@react-three/drei";
import type { ThreeEvent } from "@react-three/fiber";
import type { Event } from "@/types/event";
import { latLonToVector3 } from "@/components/globe/geo";
import { severityToColor } from "@/components/globe/severity-colors";

interface EventMarkersProps {
  events: Event[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}

/**
 * Relative marker scale derived from impact (0–100). Bigger, higher-impact
 * events read as larger dots so the globe conveys "where + how significant"
 * at a glance; a floor keeps minor events tappable.
 */
function impactScale(impact: number): number {
  return 0.65 + (Math.min(Math.max(impact, 0), 100) / 100) * 1.35;
}

/** Instanced event markers positioned on the globe surface, sized by impact. */
export function EventMarkers({ events, selectedId, onSelect }: EventMarkersProps) {
  const limit = Math.max(events.length, 1);

  const positions = useMemo(
    () =>
      events.map((event) =>
        latLonToVector3(event.location.lat, event.location.lon),
      ),
    [events],
  );

  const setHoverCursor = useCallback((hovering: boolean) => {
    document.body.style.cursor = hovering ? "pointer" : "";
  }, []);

  if (events.length === 0) {
    return null;
  }

  return (
    <Instances
      limit={limit}
      range={events.length}
      onPointerOver={() => setHoverCursor(true)}
      onPointerOut={() => setHoverCursor(false)}
    >
      <sphereGeometry args={[0.024, 12, 12]} />
      <meshBasicMaterial toneMapped={false} />
      {events.map((event, index) => {
        const selected = event.id === selectedId;
        const scale = impactScale(event.impact_score) * (selected ? 1.7 : 1);
        return (
          <Instance
            key={event.id}
            position={positions[index]}
            scale={scale}
            color={severityToColor(event.severity)}
            onClick={(clickEvent: ThreeEvent<MouseEvent>) => {
              clickEvent.stopPropagation();
              onSelect(event.id);
            }}
          />
        );
      })}
    </Instances>
  );
}
