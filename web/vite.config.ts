import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

import compactLayout from "./postcss-compact-layout.mjs";

export default defineConfig({
  css: { postcss: { plugins: [compactLayout()] } },
  plugins: [react()],
  // One React per page even if node_modules holds a nested node_modules link to another checkout
  // (pre-bundled react-dom with a second React = "Invalid hook call").
  resolve: {
    dedupe: ["react", "react-dom"],
    // The module shared with web2 (`web-shared/AGENTS.md`).
    alias: { "@ti4/shared": fileURLToPath(new URL("../web-shared/src", import.meta.url)) },
  },
  server: {
    // The repo root: `web-shared/` is outside this app.
    fs: { allow: [".."] },
    host: "127.0.0.1",
    port: 3000,
    proxy: {
      "/api": `http://127.0.0.1:${process.env.TI4_E2E_BACKEND_PORT ?? "8080"}`,
      "/ws": {
        target: `ws://127.0.0.1:${process.env.TI4_E2E_BACKEND_PORT ?? "8080"}`,
        ws: true,
      },
    },
  },
  test: {
    globals: true,
    environment: "jsdom",
    pool: "vmThreads",
    setupFiles: "./src/test/setup.ts",
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
