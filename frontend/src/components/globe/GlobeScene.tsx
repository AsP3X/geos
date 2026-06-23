import { useMemo } from "react";
import { Stars } from "@react-three/drei";
import { GlobeEarth } from "@/components/globe/GlobeEarth";
import { GlobeCountryBorders } from "@/components/globe/GlobeCountryBorders";
import { EventMarkers } from "@/components/globe/EventMarkers";
import { QuakeHeatLayer } from "@/components/globe/QuakeHeatLayer";
import { type GlobeLayers, isQuake } from "@/components/globe/layers";
import { GLOBE_RADIUS } from "@/components/globe/geo";
import type { Event } from "@/types/event";

interface GlobeSceneProps {
  events: Event[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  layers: GlobeLayers;
}

/** r3f scene graph: starfield, terminator lighting, earth, and event layers. */
export function GlobeScene({ events, selectedId, onSelect, layers }: GlobeSceneProps) {
  const quakeEvents = useMemo(() => events.filter(isQuake), [events]);

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
      <GlobeCountryBorders />
      {layers.quakeHeat ? <QuakeHeatLayer events={quakeEvents} /> : null}
      {layers.quakeDots ? (
        <EventMarkers events={quakeEvents} selectedId={selectedId} onSelect={onSelect} />
      ) : null}
    </>
  );
}
