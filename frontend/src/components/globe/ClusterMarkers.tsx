import { useMemo, useRef } from "react";
import * as THREE from "three";
import { Html } from "@react-three/drei";
import { useFrame, useThree } from "@react-three/fiber";
import { MARKER_BASE_RADIUS, ORBIT_MAX_DISTANCE, ORBIT_MIN_DISTANCE } from "@/components/globe/geo";
import { ScreenScaledInstances, type ScaledItem } from "@/components/globe/ScreenScaledInstances";
import type { GlobeCluster } from "@/components/globe/useScreenClusters";

interface ClusterMarkersProps {
  clusters: GlobeCluster[];
  /** Open a cluster's members in the sidebar list for disambiguation. */
  onSelectCluster: (cluster: GlobeCluster) => void;
}

/** Cap on count labels to bound DOM nodes; densest clusters are labeled first. */
const MAX_LABELS = 70;
/** Fixed instance capacity; the draw count tracks the active clusters. */
const CLUSTER_CAP = 4_000;
/** Screen-radius clamp (CSS px) for cluster bubbles. */
const CLUSTER_MIN_PX = 7;
const CLUSTER_MAX_PX = 26;

/** Bubble world radius grows sub-linearly with member count (before clamping). */
function clusterWorld(count: number): number {
  return MARKER_BASE_RADIUS * (1.6 + Math.log10(count) * 1.1);
}

function formatCount(count: number): string {
  return count >= 1000 ? `${(count / 1000).toFixed(count >= 10_000 ? 0 : 1)}k` : String(count);
}

/**
 * Cluster bubbles + count labels. Clicking a bubble dollies the camera in (the
 * cluster splits as density drops) and opens its members in the sidebar list so
 * an individual quake can be picked — no on-globe radial menu.
 */
export function ClusterMarkers({ clusters, onSelectCluster }: ClusterMarkersProps) {
  const camera = useThree((state) => state.camera);

  // Lightweight dolly tween toward a clicked cluster.
  const tweenRef = useRef<{
    active: boolean;
    from: THREE.Vector3;
    to: THREE.Vector3;
    t: number;
  }>({ active: false, from: new THREE.Vector3(), to: new THREE.Vector3(), t: 0 });

  const items = useMemo<ScaledItem[]>(
    () =>
      clusters.slice(0, CLUSTER_CAP).map((cluster) => ({
        id: cluster.id,
        position: cluster.position,
        color: cluster.color,
        baseWorld: clusterWorld(cluster.count),
      })),
    [clusters],
  );

  const labelClusters = useMemo(() => {
    if (clusters.length <= MAX_LABELS) {
      return clusters;
    }
    return clusters.slice().sort((a, b) => b.count - a.count).slice(0, MAX_LABELS);
  }, [clusters]);

  useFrame((_, delta) => {
    const tween = tweenRef.current;
    if (!tween.active) {
      return;
    }
    tween.t = Math.min(1, tween.t + delta * 3.2);
    const eased = 1 - (1 - tween.t) * (1 - tween.t);
    camera.position.lerpVectors(tween.from, tween.to, eased);
    if (tween.t >= 1) {
      tween.active = false;
    }
  });

  const zoomToCluster = (cluster: GlobeCluster) => {
    const currentDistance = camera.position.length();
    const targetDistance = THREE.MathUtils.clamp(
      currentDistance * 0.5,
      ORBIT_MIN_DISTANCE,
      ORBIT_MAX_DISTANCE,
    );
    const direction = cluster.position.clone().normalize();
    tweenRef.current = {
      active: true,
      from: camera.position.clone(),
      to: direction.multiplyScalar(targetDistance),
      t: 0,
    };
  };

  if (clusters.length === 0) {
    return null;
  }

  return (
    <>
      <ScreenScaledInstances
        items={items}
        capacity={CLUSTER_CAP}
        segments={14}
        minPx={CLUSTER_MIN_PX}
        maxPx={CLUSTER_MAX_PX}
        opacity={0.92}
        depthWrite={false}
        renderOrder={3}
        onClickItem={(id) => {
          const cluster = clusters.find((entry) => entry.id === id);
          if (cluster) {
            onSelectCluster(cluster);
            zoomToCluster(cluster);
          }
        }}
      />

      {labelClusters.map((cluster) => (
        <Html
          key={cluster.id}
          position={cluster.position}
          center
          zIndexRange={[20, 0]}
          style={{ pointerEvents: "none", userSelect: "none" }}
        >
          <span
            className="rounded-full bg-black/55 px-1.5 py-0.5 text-[10px] font-semibold tabular-nums text-white shadow-sm"
            style={{ textShadow: "0 1px 2px rgba(0,0,0,0.7)" }}
          >
            {formatCount(cluster.count)}
          </span>
        </Html>
      ))}
    </>
  );
}
