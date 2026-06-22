import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

/** Backend for `/api` proxy (dev + preview). Compose sets `http://api:8080`. */
const apiProxyTarget = process.env.VITE_API_PROXY_TARGET ?? "http://localhost:8080";

const apiProxy = {
  target: apiProxyTarget,
  changeOrigin: true,
  ws: true,
};

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  server: {
    port: 5173,
    proxy: {
      "/api": apiProxy,
    },
  },
  preview: {
    port: 4173,
    proxy: {
      "/api": apiProxy,
    },
  },
});
