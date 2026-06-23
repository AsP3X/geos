import { useMemo } from "react";
import * as THREE from "three";
import { useThree } from "@react-three/fiber";
import { useTexture } from "@react-three/drei";
import { GLOBE_RADIUS } from "@/components/globe/geo";

const EARTH_TEXTURE_URL = "/textures/earth_daymap.jpg";

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

/** Photographic (Blue Marble) earth with layered atmospheric glow. */
export function GlobeEarth() {
  const maxAnisotropy = useThree((state) => state.gl.capabilities.getMaxAnisotropy());
  // Configure the texture on load (mutating inside the hook keeps lint happy and
  // gives crisp imagery at grazing angles / deep zoom; mipmaps avoid shimmer).
  const texture = useTexture(EARTH_TEXTURE_URL, (loaded) => {
    const tex = Array.isArray(loaded) ? loaded[0] : loaded;
    tex.colorSpace = THREE.SRGBColorSpace;
    tex.anisotropy = maxAnisotropy;
    tex.minFilter = THREE.LinearMipmapLinearFilter;
    tex.magFilter = THREE.LinearFilter;
    tex.generateMipmaps = true;
    tex.needsUpdate = true;
  });

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
        <sphereGeometry args={[GLOBE_RADIUS, 128, 128]} />
        <meshStandardMaterial
          map={texture}
          emissiveMap={texture}
          color="#ffffff"
          emissive="#ffffff"
          emissiveIntensity={0.22}
          roughness={0.85}
          metalness={0.05}
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
