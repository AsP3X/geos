import * as THREE from "three";
import type { Event } from "@/types/event";

const SEVERITY_COLORS: Record<Event["severity"], THREE.Color> = {
  info: new THREE.Color("#94a3b8"),
  low: new THREE.Color("#38bdf8"),
  moderate: new THREE.Color("#f59e0b"),
  high: new THREE.Color("#f97316"),
  critical: new THREE.Color("#ef4444"),
};

/** Map event severity to a cached Three.js color. */
export function severityToColor(severity: Event["severity"]): THREE.Color {
  return SEVERITY_COLORS[severity];
}
