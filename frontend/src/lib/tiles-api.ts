import { apiGet } from "@/lib/api-client";
import { apiBaseUrl } from "@/lib/env";

/** Response from `GET /api/v1/tiles/session`. */
export interface TilesSession {
  token: string;
  expires_in: number;
  /**
   * Server-enforced imagery zoom ceiling (`tiles_max_zoom`). The globe caps
   * Cesium's `maximumLevel` to this so it never requests tiles deeper than the
   * configured limit — lowering it on storage-constrained deployments reduces
   * how many tiles the proxy caches.
   */
  max_zoom: number;
}

/** Fetch a medium-lived tiles token for the Cesium imagery provider. */
export async function getTilesSession(accessToken: string): Promise<TilesSession> {
  return apiGet<TilesSession>("/api/v1/tiles/session", accessToken);
}

/**
 * Build the Sentinel-2 imagery URL template for Cesium's
 * `UrlTemplateImageryProvider`, with the tiles token embedded. Cesium expands
 * the `{z}`/`{x}`/`{y}` placeholders per requested tile.
 */
export function sentinel2TemplateUrl(token: string): string {
  const base = apiBaseUrl();
  return `${base}/api/v1/tiles/sentinel2/{z}/{x}/{y}.jpg?token=${encodeURIComponent(token)}`;
}
