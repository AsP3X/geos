/// <reference lib="webworker" />

/**
 * Clustering Web Worker: keeps the loaded quake dataset off the main thread and
 * answers view-scoped cluster queries so binning ~100k points never blocks
 * rendering/interaction. Full grids are cached per LOD level; each query just
 * culls the cached grid to the camera view and returns the visible subset
 * (single indices + clusters) back to the main thread.
 */

import {
  type ClusterDataset,
  type ClusterRequest,
  type ClusterResultMessage,
  type GridResult,
  buildFullGrid,
  filterGridToView,
} from "@/components/globe/cesium/cluster-grid";

const ctx = self as unknown as DedicatedWorkerGlobalScope;

let dataset: ClusterDataset | null = null;
let dataVersion = -1;
/** Full (view-independent) grids cached per `levelKey` for the current dataset. */
const gridCache = new Map<number, GridResult>();

ctx.onmessage = (event: MessageEvent<ClusterRequest>) => {
  const message = event.data;

  if (message.type === "data") {
    dataset = {
      lon: new Float64Array(message.lon),
      lat: new Float64Array(message.lat),
      severityRank: new Uint8Array(message.severityRank),
      impact: new Float32Array(message.impact),
      count: message.count,
    };
    dataVersion = message.version;
    gridCache.clear();
    return;
  }

  // query — ignore if the dataset for this version hasn't been applied yet.
  if (!dataset || message.version !== dataVersion) {
    return;
  }

  let grid = gridCache.get(message.levelKey);
  if (!grid) {
    grid = buildFullGrid(dataset, message.cellDeg, message.uncluster);
    gridCache.set(message.levelKey, grid);
  }
  const visible = filterGridToView(dataset, grid, message.view);

  const singleIndices = Uint32Array.from(visible.singleIndices);
  const result: ClusterResultMessage = {
    type: "result",
    version: message.version,
    requestId: message.requestId,
    levelKey: message.levelKey,
    singleIndices,
    clusters: visible.clusters,
  };
  ctx.postMessage(result, [singleIndices.buffer]);
};
