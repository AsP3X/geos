/// Base URL for the Geos HTTP API (no trailing slash).
/// Empty string uses same-origin (Vite dev proxy → API).
export function apiBaseUrl(): string {
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
