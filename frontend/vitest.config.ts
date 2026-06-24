import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vitest/config";

// Standalone config (not the Vite app config) so unit tests run under a plain
// Node environment without loading the Cesium plugin or pulling in WebGL/DOM.
// The `@` alias mirrors vite.config.ts / tsconfig so test imports resolve.
export default defineConfig({
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
