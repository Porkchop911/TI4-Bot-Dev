import { defineConfig, devices } from "@playwright/test";

// The production build under the exact CSP of deploy/public/Caddyfile (see csp_strict.spec.ts).
// No backend: a tiny static server inside the spec serves the build with the header.
//   cd web && npx playwright test -c e2e/csp_strict.config.ts
export default defineConfig({
  testDir: ".",
  testMatch: /csp_strict\.spec\.ts$/,
  outputDir: process.env.TI4_E2E_OUTPUT ?? "../test-results",
  timeout: 120_000,
  workers: 1,
  reporter: "list",
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  globalSetup: "./csp_strict.setup.ts",
});
