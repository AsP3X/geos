/// Base URL for the Geos HTTP API (no trailing slash).
/// Empty string uses same-origin paths (`/api/v1/...`).
///
/// Resolution order:
/// 1. Runtime `window.__GEOS_CONFIG__.apiBaseUrl` from `/config.js` (Docker /
///    reverse-proxy deploys — set via `GEOS_API_BASE_URL` at container start)
/// 2. Build-time `VITE_API_BASE_URL` (local `pnpm dev` / optional bake-in)
/// 3. Same-origin (empty)
export function apiBaseUrl(): string {
  const runtime = window.__GEOS_CONFIG__?.apiBaseUrl;
  if (typeof runtime === "string" && runtime.length > 0) {
    return runtime.replace(/\/$/, "");
  }
  const configured = import.meta.env.VITE_API_BASE_URL;
  if (typeof configured === "string" && configured.length > 0) {
    return configured.replace(/\/$/, "");
  }
  return "";
}

/// WebSocket base derived from the HTTP API URL.
export function streamBaseUrl(): string {
  const http = apiBaseUrl();
  if (http.length === 0) {
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    return `${protocol}//${window.location.host}`;
  }
  if (http.startsWith("https://")) {
    return `wss://${http.slice("https://".length)}`;
  }
  if (http.startsWith("http://")) {
    return `ws://${http.slice("http://".length)}`;
  }
  return http;
}
