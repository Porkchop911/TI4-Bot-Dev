import { test } from "@playwright/test";
import { openInspector, capture, mine, ground, fleet } from "./scene";

test("2-phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 1300 });
  await openInspector(page, [
    ...fleet,
    ground("jord"),
    mine("jord", "pds"),
    mine("jord", "spacedock"),
    mine("exhausted", "pds"),
  ]);
  await capture(page, testInfo, "2-phone");
});
