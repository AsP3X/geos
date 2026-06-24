import { describe, expect, it } from "vitest";
import {
  FINEST_CELL_DEG,
  LEVEL_CELL_DEG,
  LEVEL_HYSTERESIS_RATIO,
  TARGET_CELL_PX,
  UNCLUSTER_LEVEL,
  desiredCellDeg,
  dotPixelScaleForHeight,
  pickLevelIndex,
  pickLevelIndexWithHysteresis,
} from "@/components/globe/cesium/lod";

describe("LEVEL_CELL_DEG", () => {
  it("is ordered coarse → fine (strictly descending)", () => {
    for (let i = 1; i < LEVEL_CELL_DEG.length; i += 1) {
      expect(LEVEL_CELL_DEG[i]).toBeLessThan(LEVEL_CELL_DEG[i - 1]);
    }
  });

  it("halves at each finer step so grids stay dyadic/aligned", () => {
    for (let i = 1; i < LEVEL_CELL_DEG.length; i += 1) {
      expect(LEVEL_CELL_DEG[i]).toBeCloseTo(LEVEL_CELL_DEG[i - 1] / 2, 12);
    }
  });

  it("bottoms out at the finest configured cell size", () => {
    expect(LEVEL_CELL_DEG[LEVEL_CELL_DEG.length - 1]).toBeCloseTo(FINEST_CELL_DEG, 12);
  });

  it("uses the array length as the uncluster sentinel level", () => {
    expect(UNCLUSTER_LEVEL).toBe(LEVEL_CELL_DEG.length);
  });
});

describe("desiredCellDeg", () => {
  const FOVY = Math.PI / 3; // 60° vertical field of view
  const CANVAS = 1000;

  it("falls back to the coarsest level for a non-positive canvas height", () => {
    expect(desiredCellDeg(1e6, FOVY, 0)).toBe(LEVEL_CELL_DEG[0]);
    expect(desiredCellDeg(1e6, FOVY, -10)).toBe(LEVEL_CELL_DEG[0]);
  });

  it("matches the camera-height → ground-cell projection", () => {
    const height = 1_000_000;
    const groundExtent = 2 * height * Math.tan(FOVY / 2);
    const metersPerPixel = groundExtent / CANVAS;
    const expectedDeg = (TARGET_CELL_PX * metersPerPixel) / 111_320;
    expect(desiredCellDeg(height, FOVY, CANVAS)).toBeCloseTo(expectedDeg, 9);
  });

  it("grows as the camera rises", () => {
    const low = desiredCellDeg(2e5, FOVY, CANVAS);
    const mid = desiredCellDeg(2e6, FOVY, CANVAS);
    const high = desiredCellDeg(2e7, FOVY, CANVAS);
    expect(mid).toBeGreaterThan(low);
    expect(high).toBeGreaterThan(mid);
  });

  it("shrinks the cell as the canvas grows taller (more pixels per cell)", () => {
    const small = desiredCellDeg(1e6, FOVY, 500);
    const large = desiredCellDeg(1e6, FOVY, 2000);
    expect(large).toBeLessThan(small);
  });
});

describe("pickLevelIndex", () => {
  it("returns the exact level when the desired size equals a level size", () => {
    for (let i = 0; i < LEVEL_CELL_DEG.length; i += 1) {
      expect(pickLevelIndex(LEVEL_CELL_DEG[i])).toBe(i);
    }
  });

  it("clamps to the coarsest level for very large desired sizes", () => {
    expect(pickLevelIndex(LEVEL_CELL_DEG[0] * 1000)).toBe(0);
  });

  it("clamps to the finest level for very small desired sizes", () => {
    const finest = LEVEL_CELL_DEG.length - 1;
    expect(pickLevelIndex(LEVEL_CELL_DEG[finest] / 1000)).toBe(finest);
  });

  it("snaps to the nearest level in log space across a boundary", () => {
    // Either side of the geometric mean of two adjacent levels resolves to the
    // closer (in log space) neighbour. The exact midpoint is a float-dependent
    // tie, so assert the meaningful behaviour just off-centre.
    const i = 5;
    const boundary = Math.sqrt(LEVEL_CELL_DEG[i - 1] * LEVEL_CELL_DEG[i]);
    expect(pickLevelIndex(boundary * 1.01)).toBe(i - 1); // toward the coarser level
    expect(pickLevelIndex(boundary * 0.99)).toBe(i); // toward the finer level
  });
});

describe("pickLevelIndexWithHysteresis", () => {
  const i = 5; // a mid-range level with neighbours on both sides
  const boundary = Math.sqrt(LEVEL_CELL_DEG[i - 1] * LEVEL_CELL_DEG[i]);

  it("returns the raw target when there is no current level", () => {
    expect(pickLevelIndexWithHysteresis(LEVEL_CELL_DEG[i], null)).toBe(
      pickLevelIndex(LEVEL_CELL_DEG[i]),
    );
  });

  it("always honours the raw target while uncluster mode is active", () => {
    const target = pickLevelIndex(LEVEL_CELL_DEG[i]);
    expect(pickLevelIndexWithHysteresis(LEVEL_CELL_DEG[i], UNCLUSTER_LEVEL)).toBe(target);
  });

  it("sticks to the finer current level inside the coarsen band (zooming out)", () => {
    // Just past the boundary the raw target is the coarser level, but we should
    // hold the current finer level until the view crosses boundary × ratio.
    const justPast = boundary * 1.05;
    expect(pickLevelIndex(justPast)).toBe(i - 1);
    expect(pickLevelIndexWithHysteresis(justPast, i)).toBe(i);
  });

  it("flips coarser once the view exceeds the hysteresis band", () => {
    const wellPast = boundary * (LEVEL_HYSTERESIS_RATIO + 0.1);
    expect(pickLevelIndexWithHysteresis(wellPast, i)).toBe(i - 1);
  });

  it("sticks to the coarser current level inside the refine band (zooming in)", () => {
    // Just below the boundary the raw target is the finer level, but we hold the
    // current coarser level until the view drops under boundary ÷ ratio.
    const justBelow = boundary * 0.95;
    expect(pickLevelIndex(justBelow)).toBe(i);
    expect(pickLevelIndexWithHysteresis(justBelow, i - 1)).toBe(i - 1);
  });

  it("flips finer once the view drops below the hysteresis band", () => {
    const wellBelow = boundary / (LEVEL_HYSTERESIS_RATIO + 0.1);
    expect(pickLevelIndexWithHysteresis(wellBelow, i - 1)).toBe(i);
  });
});

describe("dotPixelScaleForHeight", () => {
  const MAX = 2.5e7;
  const CLOSE = 0.85;
  const REGIONAL = 1.5;

  it("uses the close scale at and below the near threshold", () => {
    expect(dotPixelScaleForHeight(0, MAX)).toBeCloseTo(CLOSE, 6);
    expect(dotPixelScaleForHeight(3.0e5, MAX)).toBeCloseTo(CLOSE, 6);
  });

  it("eases back to the planet scale at and beyond max zoom", () => {
    expect(dotPixelScaleForHeight(MAX, MAX)).toBeCloseTo(CLOSE, 6);
    expect(dotPixelScaleForHeight(MAX * 2, MAX)).toBeCloseTo(CLOSE, 6);
  });

  it("peaks at the regional scale in the mid band", () => {
    expect(dotPixelScaleForHeight(MAX * 0.42, MAX)).toBeCloseTo(REGIONAL, 6);
    expect(dotPixelScaleForHeight(MAX * 0.5, MAX)).toBeCloseTo(REGIONAL, 6);
  });

  it("ramps up monotonically from close to regional", () => {
    const samples = [3.0e5, 1e6, 3e6, 6e6, MAX * 0.42];
    for (let k = 1; k < samples.length; k += 1) {
      expect(dotPixelScaleForHeight(samples[k], MAX)).toBeGreaterThanOrEqual(
        dotPixelScaleForHeight(samples[k - 1], MAX),
      );
    }
  });

  it("ramps down monotonically from regional back to planet", () => {
    const samples = [MAX * 0.7, MAX * 0.8, MAX * 0.9, MAX];
    for (let k = 1; k < samples.length; k += 1) {
      expect(dotPixelScaleForHeight(samples[k], MAX)).toBeLessThanOrEqual(
        dotPixelScaleForHeight(samples[k - 1], MAX),
      );
    }
  });

  it("stays within the [close, regional] envelope across the whole range", () => {
    for (let h = 0; h <= MAX * 1.5; h += MAX / 50) {
      const scale = dotPixelScaleForHeight(h, MAX);
      expect(scale).toBeGreaterThanOrEqual(CLOSE - 1e-9);
      expect(scale).toBeLessThanOrEqual(REGIONAL + 1e-9);
    }
  });
});
