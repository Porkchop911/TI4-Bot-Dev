import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // Not the default ".vite": Cloudflare still holds stale bundles under that path (see server.headers).
  // Cleanup: after a purge of the Cloudflare cache for this host, remove this line.
  cacheDir: "node_modules/.vite-web2",
  // The module shared with web/ (`web-shared/AGENTS.md`).
  resolve: {
    alias: { "@ti4/shared": fileURLToPath(new URL("../web-shared/src", import.meta.url)) },
  },
  server: {
    // The repo root: `web-shared/` is outside this app.
    fs: { allow: [".."] },
    allowedHosts: ["web2.beckermann-zibert.com"],
    // Vite marks pre-bundled dependencies "immutable". Cloudflare in front of Caddy then keeps a
    // stale bundle across restarts, and the page loads two copies of React. This header stops the
    // CDN only; the browser still caches.
    // Cleanup: after Cloudflare has a cache rule that bypasses the cache for this host, remove it.
    headers: { "CDN-Cache-Control": "no-store" },
    host: "127.0.0.1",
    port: 3100,
    // A game on the server is at the address of the page: `/api/...` and `/ws/games/<id>`.
    proxy: {
      "/api": `http://127.0.0.1:${process.env.TI4_E2E_BACKEND_PORT ?? "8080"}`,
      "/ws": {
        target: `ws://127.0.0.1:${process.env.TI4_E2E_BACKEND_PORT ?? "8080"}`,
        ws: true,
      },
    },
  },
  test: {
    // Two kinds of test. A `*.test.ts` has no DOM and runs in node, as the engine does. A
    // `*.test.tsx` renders components in jsdom; the VM pool spares it a process for each file.
    projects: [
      {
        extends: true,
        test: { name: "unit", include: ["src/**/*.test.ts", "../web-shared/src/**/*.test.ts"] },
      },
      {
        extends: true,
        test: {
          name: "dom",
          include: ["src/**/*.test.tsx"],
          environment: "jsdom",
          pool: "vmThreads",
          setupFiles: "./src/test/setup.ts",
        },
      },
    ],
  },
});
