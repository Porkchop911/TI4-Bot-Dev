import { test } from "@playwright/test";
import { openInspector, capture, mine, ground, fleet } from "./scene";

// Jord: PDS + space dock + infantry. Exhausted World: PDS only.
test("1-desktop", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 1100 });
  await openInspector(page, [
    ...fleet,
    ground("jord"),
    ground("jord"),
    mine("jord", "pds"),
    mine("jord", "spacedock"),
    mine("exhausted", "pds"),
  ]);
  await capture(page, testInfo, "1-desktop");
});
