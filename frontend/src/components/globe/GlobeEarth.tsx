import { useEffect, useMemo, useRef, useState } from "react";
import * as THREE from "three";
import { useThree } from "@react-three/fiber";
import { GLOBE_RADIUS } from "@/components/globe/geo";
import {
  buildLandTexture,
  type GeoJsonFeatureCollection,
} from "@/components/globe/build-map-texture";

const LAND_GEOJSON_URL = "/geo/ne_110m_land.geojson";

const ATMOSPHERE_VERTEX = /* glsl */ `
varying vec3 vNormal;
varying vec3 vEye;

void main() {
  vNormal = normalize(normalMatrix * normal);
  vec4 mvPosition = modelViewMatrix * vec4(position, 1.0);
  vEye = normalize(-mvPosition.xyz);
  gl_Position = projectionMatrix * mvPosition;
}
`;

// Outer halo: glows brightest at the silhouette (back side sphere).
const ATMOSPHERE_OUTER_FRAGMENT = /* glsl */ `
uniform vec3 glowColor;
uniform float power;
uniform float strength;
varying vec3 vNormal;
varying vec3 vEye;

void main() {
  float rim = pow(1.0 - abs(dot(vNormal, vEye)), power);
  gl_FragColor = vec4(glowColor, rim * strength);
}
`;

// Inner glow: faint atmospheric scatter across the lit face toward the rim.
const ATMOSPHERE_INNER_FRAGMENT = /* glsl */ `
uniform vec3 glowColor;
uniform float power;
uniform float strength;
varying vec3 vNormal;
varying vec3 vEye;

void main() {
  float rim = pow(1.0 - max(dot(vNormal, vEye), 0.0), power);
  gl_FragColor = vec4(glowColor, rim * strength);
}
`;

/** Textured earth (painted land on ocean) with layered atmospheric glow. */
export function GlobeEarth() {
  const maxAnisotropy = useThree((state) => state.gl.capabilities.getMaxAnisotropy());
  const [texture, setTexture] = useState<THREE.CanvasTexture | null>(null);
  const textureRef = useRef<THREE.CanvasTexture | null>(null);

  useEffect(() => {
    let cancelled = false;

    void fetch(LAND_GEOJSON_URL)
      .then((response) => {
        if (!response.ok) {
          throw new Error(`failed to load land geometry (${response.status})`);
        }
        return response.json() as Promise<GeoJsonFeatureCollection>;
      })
      .then((data) => {
        if (cancelled) {
          return;
        }
        const nextTexture = buildLandTexture(data, { anisotropy: maxAnisotropy });
        textureRef.current?.dispose();
        textureRef.current = nextTexture;
        setTexture(nextTexture);
      })
      .catch(() => {
        // Surface texture is optional; the ocean color still renders.
      });

    return () => {
      cancelled = true;
      textureRef.current?.dispose();
      textureRef.current = null;
    };
  }, [maxAnisotropy]);

  const outerUniforms = useMemo(
    () => ({
      glowColor: { value: new THREE.Color("#3da9ff") },
      power: { value: 5.5 },
      strength: { value: 0.85 },
    }),
    [],
  );

  const innerUniforms = useMemo(
    () => ({
      glowColor: { value: new THREE.Color("#1d6fd0") },
      power: { value: 4.5 },
      strength: { value: 0.35 },
    }),
    [],
  );

  return (
    <group>
      <mesh renderOrder={0}>
        <sphereGeometry args={[GLOBE_RADIUS, 96, 96]} />
        <meshStandardMaterial
          // Remount the material when the texture arrives so the map compiles in.
          key={texture ? "textured" : "plain"}
          map={texture}
          emissiveMap={texture}
          color={texture ? "#ffffff" : "#0b2742"}
          emissive={texture ? "#ffffff" : "#06182b"}
          emissiveIntensity={texture ? 0.28 : 0.5}
          roughness={0.72}
          metalness={0.12}
        />
      </mesh>

      {/* Inner atmosphere scatter on the front face. */}
      <mesh scale={1.004} renderOrder={4}>
        <sphereGeometry args={[GLOBE_RADIUS, 64, 64]} />
        <shaderMaterial
          transparent
          depthWrite={false}
          side={THREE.FrontSide}
          blending={THREE.AdditiveBlending}
          uniforms={innerUniforms}
          vertexShader={ATMOSPHERE_VERTEX}
          fragmentShader={ATMOSPHERE_INNER_FRAGMENT}
        />
      </mesh>

      {/* Outer halo behind the globe silhouette. */}
      <mesh scale={1.06} renderOrder={5}>
        <sphereGeometry args={[GLOBE_RADIUS, 64, 64]} />
        <shaderMaterial
          transparent
          depthWrite={false}
          side={THREE.BackSide}
          blending={THREE.AdditiveBlending}
          uniforms={outerUniforms}
          vertexShader={ATMOSPHERE_VERTEX}
          fragmentShader={ATMOSPHERE_OUTER_FRAGMENT}
        />
      </mesh>
    </group>
  );
}
