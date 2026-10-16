import { test } from "@playwright/test";
import { shot } from "../_shared/shot";

// The gallery is a plain vite page (gallery.html) that draws every candidate through StructureIcon.
test("4-icon-gallery", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 920, height: 900 });
  await page.goto("/e2e/screenshots/BA-system-structures/gallery.html");
  await page.locator("#root > div").waitFor();
  await shot(page, testInfo, "after-4-icon-gallery", { of: page.locator("#root > div") });
});
