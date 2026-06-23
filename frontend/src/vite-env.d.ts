/// <reference types="vite/client" />

interface GeosRuntimeConfig {
  apiBaseUrl?: string;
}

interface Window {
  __GEOS_CONFIG__?: GeosRuntimeConfig;
}
