import { describe, expect, it } from "vitest";
import {
  BBOX_CHANGE_TOLERANCE_RATIO,
  VIEWPORT_MARGIN,
  WHOLE_GLOBE_WIDTH_DEG,
  bboxChanged,
  viewBoundsToBBox,
} from "@/components/globe/cesium/viewport";

describe("viewBoundsToBBox", () => {
  it("expands the view by the margin and keeps it within world bounds", () => {
    const bbox = viewBoundsToBBox(0, 0, 10, 10);
    expect(bbox).not.toBeNull();
    // 10° span * 0.15 margin = 1.5° on each edge.
    expect(bbox?.minLon).toBeCloseTo(-1.5, 6);
    expect(bbox?.maxLon).toBeCloseTo(11.5, 6);
    expect(bbox?.minLat).toBeCloseTo(-1.5, 6);
    expect(bbox?.maxLat).toBeCloseTo(11.5, 6);
    expect(VIEWPORT_MARGIN).toBe(0.15);
  });

  it("clamps the expanded bbox to [-180,180] / [-90,90]", () => {
    const bbox = viewBoundsToBBox(-179, -89, -120, 80);
    expect(bbox?.minLon).toBe(-180);
    expect(bbox?.minLat).toBe(-90);
    expect(bbox?.maxLat).toBeLessThanOrEqual(90);
  });

  it("returns null for antimeridian-wrapping views (east <= west)", () => {
    expect(viewBoundsToBBox(170, -10, -170, 10)).toBeNull();
    expect(viewBoundsToBBox(50, 0, 50, 10)).toBeNull();
  });

  it("returns null for near-global width (load globally)", () => {
    expect(viewBoundsToBBox(-180, -85, 180, 85)).toBeNull();
    expect(viewBoundsToBBox(-180, -85, WHOLE_GLOBE_WIDTH_DEG - 180, 85)).toBeNull();
  });
});

describe("bboxChanged", () => {
  const region = { minLon: 0, minLat: 0, maxLon: 10, maxLat: 10 };

  it("treats staying global (null <-> null) as unchanged", () => {
    expect(bboxChanged(null, null)).toBe(false);
  });

  it("treats null <-> bbox transitions as changed", () => {
    expect(bboxChanged(null, region)).toBe(true);
    expect(bboxChanged(region, null)).toBe(true);
  });

  it("ignores sub-tolerance pans within the same region", () => {
    // 10° span * 0.12 tolerance = 1.2°; a 0.5° nudge stays put.
    const nudged = { minLon: 0.5, minLat: 0.5, maxLon: 10.5, maxLat: 10.5 };
    expect(bboxChanged(region, nudged)).toBe(false);
    expect(BBOX_CHANGE_TOLERANCE_RATIO).toBe(0.12);
  });

  it("detects pans beyond the tolerance", () => {
    const moved = { minLon: 3, minLat: 0, maxLon: 13, maxLat: 10 };
    expect(bboxChanged(region, moved)).toBe(true);
  });
});
