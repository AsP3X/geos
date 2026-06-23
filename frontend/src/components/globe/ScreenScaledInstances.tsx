import { useEffect, useMemo, useRef } from "react";
import * as THREE from "three";
import { useFrame, useThree, type ThreeEvent } from "@react-three/fiber";
import { MARKER_BASE_RADIUS, MARKER_SURFACE_RADIUS } from "@/components/globe/geo";

/** One sphere instance to render on the globe. */
export interface ScaledItem {
  id: string;
  position: THREE.Vector3;
  color: THREE.Color | string;
  /** Natural world radius (drives mid-zoom size before clamping). */
  baseWorld: number;
}

interface ScreenScaledInstancesProps {
  items: ScaledItem[];
  /** Fixed buffer capacity; only `items.length` instances are drawn. */
  capacity: number;
  segments: number;
  /** Clamp on the rendered screen radius (CSS px) at both zoom extremes. */
  minPx: number;
  maxPx: number;
  opacity?: number;
  depthWrite?: boolean;
  renderOrder?: number;
  onSelectItem?: (id: string) => void;
  onClickItem?: (id: string, event: ThreeEvent<MouseEvent>) => void;
  onHoverItem?: (id: string, event: ThreeEvent<PointerEvent>) => void;
  onHoverOut?: () => void;
}

const tmpColor = new THREE.Color();
const cullFrustum = new THREE.Frustum();
const cullMatrix = new THREE.Matrix4();

/**
 * Instanced 3D spheres whose on-screen size is clamped. Each sphere uses its
 * natural world radius (so size tracks zoom in the mid range) but is rescaled
 * every frame to stay within [minPx, maxPx] of screen radius — visible when
 * zoomed out, never gigantic when zoomed in. Picking is per-instance via the
 * raycaster's `instanceId`.
 */
export function ScreenScaledInstances({
  items,
  capacity,
  segments,
  minPx,
  maxPx,
  opacity = 1,
  depthWrite = true,
  renderOrder = 2,
  onClickItem,
  onHoverItem,
  onHoverOut,
}: ScreenScaledInstancesProps) {
  const meshRef = useRef<THREE.InstancedMesh>(null);
  const size = useThree((state) => state.size);
  const dummy = useMemo(() => new THREE.Object3D(), []);

  // Change detection so an idle camera does no per-instance work at all.
  const lastMatrixRef = useRef(new THREE.Matrix4());
  const lastHeightRef = useRef(0);
  const lastItemsRef = useRef<ScaledItem[] | null>(null);

  // Push per-instance colors and prime the draw count whenever items change.
  useEffect(() => {
    const mesh = meshRef.current;
    if (!mesh) {
      return;
    }
    const count = Math.min(items.length, capacity);
    for (let i = 0; i < count; i += 1) {
      tmpColor.set(items[i].color);
      mesh.setColorAt(i, tmpColor);
    }
    mesh.count = count;
    if (mesh.instanceColor) {
      mesh.instanceColor.needsUpdate = true;
    }
  }, [items, capacity]);

  // Re-derive each sphere's world scale from camera distance so its projected
  // screen radius lands inside [minPx, maxPx]. Skipped entirely while idle, and
  // back-of-globe (occluded) instances are collapsed so they cost nothing.
  useFrame((state) => {
    const mesh = meshRef.current;
    if (!mesh) {
      return;
    }
    const camera = state.camera as THREE.PerspectiveCamera;

    const moved = !camera.matrixWorld.equals(lastMatrixRef.current);
    const resized = size.height !== lastHeightRef.current;
    const itemsChanged = items !== lastItemsRef.current;
    if (!moved && !resized && !itemsChanged) {
      return;
    }
    lastMatrixRef.current.copy(camera.matrixWorld);
    lastHeightRef.current = size.height;
    lastItemsRef.current = items;

    const count = Math.min(items.length, capacity);
    // Pixels per (worldRadius / distance) for the current viewport + fov.
    const pxPerUnit = size.height / (2 * Math.tan((camera.fov * Math.PI) / 360));
    const camPos = camera.position;
    // Horizon test: a surface point is occluded when pos·cam <= surfaceR².
    const horizon = MARKER_SURFACE_RADIUS * MARKER_SURFACE_RADIUS;
    // Off-screen instances (common when zoomed in) are collapsed too, so the GPU
    // never shades dots outside the viewport.
    cullMatrix.multiplyMatrices(camera.projectionMatrix, camera.matrixWorldInverse);
    cullFrustum.setFromProjectionMatrix(cullMatrix);

    for (let i = 0; i < count; i += 1) {
      const item = items[i];
      const pos = item.position;
      dummy.position.copy(pos);

      const occluded = pos.x * camPos.x + pos.y * camPos.y + pos.z * camPos.z <= horizon;
      if (occluded || !cullFrustum.containsPoint(pos)) {
        // Behind the globe or outside the viewport — collapse to a zero-area instance.
        dummy.scale.setScalar(0);
      } else {
        const distance = camPos.distanceTo(pos);
        const naturalPx = (item.baseWorld * pxPerUnit) / distance;
        const clampedPx = Math.min(Math.max(naturalPx, minPx), maxPx);
        const worldRadius = (clampedPx * distance) / pxPerUnit;
        dummy.scale.setScalar(worldRadius / MARKER_BASE_RADIUS);
      }
      dummy.updateMatrix();
      mesh.setMatrixAt(i, dummy.matrix);
    }
    mesh.count = count;
    mesh.instanceMatrix.needsUpdate = true;
  });

  if (items.length === 0) {
    return null;
  }

  // Only attach pointer-move/out handlers when a hover consumer exists. Otherwise
  // r3f would raycast this (large) instanced mesh on every pointer move — including
  // while dragging to rotate — which is a major, needless cost.
  const hoverHandlers = onHoverItem
    ? {
        onPointerMove: (event: ThreeEvent<PointerEvent>) => {
          if (event.instanceId == null) {
            return;
          }
          const item = items[event.instanceId];
          if (item) {
            event.stopPropagation();
            onHoverItem(item.id, event);
          }
        },
        onPointerOut: () => onHoverOut?.(),
      }
    : {};

  return (
    <instancedMesh
      ref={meshRef}
      args={[undefined, undefined, capacity]}
      frustumCulled={false}
      renderOrder={renderOrder}
      onClick={(event: ThreeEvent<MouseEvent>) => {
        if (event.instanceId == null) {
          return;
        }
        const item = items[event.instanceId];
        if (item) {
          event.stopPropagation();
          onClickItem?.(item.id, event);
        }
      }}
      {...hoverHandlers}
    >
      <sphereGeometry args={[MARKER_BASE_RADIUS, segments, segments]} />
      <meshBasicMaterial transparent={opacity < 1} opacity={opacity} depthWrite={depthWrite} toneMapped={false} />
    </instancedMesh>
  );
}
