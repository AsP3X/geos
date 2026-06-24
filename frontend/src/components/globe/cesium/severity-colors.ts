import { Color } from "cesium";
import type { Event } from "@/types/event";

/** Severity → hex for globe markers (tuned for the dark Cesium basemap). */
const SEVERITY_HEX: Record<Event["severity"], string> = {
  info: "#2dd4bf",
  low: "#38bdf8",
  moderate: "#f59e0b",
  high: "#f97316",
  critical: "#ef4444",
};

const SEVERITY_CESIUM: Record<Event["severity"], Color> = {
  info: Color.fromCssColorString(SEVERITY_HEX.info),
  low: Color.fromCssColorString(SEVERITY_HEX.low),
  moderate: Color.fromCssColorString(SEVERITY_HEX.moderate),
  high: Color.fromCssColorString(SEVERITY_HEX.high),
  critical: Color.fromCssColorString(SEVERITY_HEX.critical),
};

/** Map event severity to a cached Cesium color. */
export function severityToCesiumColor(severity: Event["severity"]): Color {
  return SEVERITY_CESIUM[severity];
}

/** Map event severity to its hex string (for labels / DOM). */
export function severityToHex(severity: Event["severity"]): string {
  return SEVERITY_HEX[severity];
}
