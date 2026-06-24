import { describe, expect, it } from "vitest";
import {
  type ClusterDataset,
  buildFullGrid,
  filterGridToView,
} from "@/components/globe/cesium/cluster-grid";

function dataset(
  points: { lon: number; lat: number; severityRank?: number; impact?: number }[],
): ClusterDataset {
  const n = points.length;
  const lon = new Float64Array(n);
  const lat = new Float64Array(n);
  const severityRank = new Uint8Array(n);
  const impact = new Float32Array(n);
  points.forEach((p, i) => {
    lon[i] = p.lon;
    lat[i] = p.lat;
    severityRank[i] = p.severityRank ?? 0;
    impact[i] = p.impact ?? 0;
  });
  return { lon, lat, severityRank, impact, count: n };
}

describe("buildFullGrid (clustered)", () => {
  it("merges nearby points into a cluster and leaves far points single", () => {
    const data = dataset([
      { lon: 0.0, lat: 0.0 },
      { lon: 0.1, lat: 0.1 },
      { lon: 50, lat: 50 },
    ]);
    const grid = buildFullGrid(data, 5, false);
    expect(grid.clusters).toHaveLength(1);
    expect(grid.clusters[0].count).toBe(2);
    expect(grid.singleIndices).toEqual([2]);
  });

  it("anchors a cluster on the highest-impact member (position + severity)", () => {
    const data = dataset([
      { lon: 0.0, lat: 0.0, severityRank: 1, impact: 10 },
      { lon: 0.2, lat: 0.2, severityRank: 4, impact: 90 },
    ]);
    const grid = buildFullGrid(data, 5, false);
    expect(grid.clusters).toHaveLength(1);
    const cluster = grid.clusters[0];
    expect(cluster.severityRank).toBe(4);
    expect(cluster.lon).toBeCloseTo(0.2, 9);
    expect(cluster.memberIndices[0]).toBe(1); // highest impact first
  });
});

describe("buildFullGrid (unclustered)", () => {
  it("returns every point as a single and no clusters", () => {
    const data = dataset([
      { lon: 0, lat: 0 },
      { lon: 0.001, lat: 0.001 },
    ]);
    const grid = buildFullGrid(data, 0.003, true);
    expect(grid.clusters).toHaveLength(0);
    expect(grid.singleIndices).toEqual([0, 1]);
  });
});

describe("filterGridToView", () => {
  const data = dataset([
    { lon: 0, lat: 0 },
    { lon: 100, lat: 40 },
  ]);

  it("returns the full grid unchanged for a null (whole-globe) view", () => {
    const grid = buildFullGrid(data, 0.003, true);
    expect(filterGridToView(data, grid, null)).toBe(grid);
  });

  it("keeps only singles inside the view rect", () => {
    const grid = buildFullGrid(data, 0.003, true);
    const visible = filterGridToView(data, grid, {
      west: -10,
      south: -10,
      east: 10,
      north: 10,
    });
    expect(visible.singleIndices).toEqual([0]);
  });

  it("culls clusters whose anchor falls outside the view", () => {
    const clustered = dataset([
      { lon: 0, lat: 0 },
      { lon: 0.2, lat: 0.2 },
      { lon: 100, lat: 40 },
      { lon: 100.2, lat: 40.2 },
    ]);
    const grid = buildFullGrid(clustered, 5, false);
    expect(grid.clusters).toHaveLength(2);
    const visible = filterGridToView(clustered, grid, {
      west: -10,
      south: -10,
      east: 10,
      north: 10,
    });
    expect(visible.clusters).toHaveLength(1);
    expect(visible.clusters[0].lon).toBeCloseTo(0, 6);
  });
});
