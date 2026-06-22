import { useMemo } from "react";
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

/** Instanced event markers positioned on the globe surface. */
export function EventMarkers({ events, selectedId, onSelect }: EventMarkersProps) {
  const limit = Math.max(events.length, 1);

  const positions = useMemo(
    () =>
      events.map((event) =>
        latLonToVector3(event.location.lat, event.location.lon),
      ),
    [events],
  );

  if (events.length === 0) {
    return null;
  }

  return (
    <Instances limit={limit} range={events.length}>
      <sphereGeometry args={[0.028, 10, 10]} />
      <meshBasicMaterial toneMapped={false} />
      {events.map((event, index) => {
        const selected = event.id === selectedId;
        return (
          <Instance
            key={event.id}
            position={positions[index]}
            scale={selected ? 1.75 : 1}
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
