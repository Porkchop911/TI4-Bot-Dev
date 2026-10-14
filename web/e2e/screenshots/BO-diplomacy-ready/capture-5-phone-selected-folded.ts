import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openReady, prefix } from "./scene";

// Phone: both planets picked, then the bar folded down to the summary and Confirm so the map is free.
// The fold is the minimise: the picks and Confirm stay. Targets are at least 44 px tall.
test.use({ viewport: { width: 390, height: 844 }, hasTouch: true });
test("diplomacy ready: phone, picked and folded", async ({ page }, testInfo) => {
  await openReady(page);
  if (prefix === "before") {
    await page.getByTestId("planet-chip-jord").tap();
    await shot(page, testInfo, `${prefix}-5a-phone-picked`);
    return;
  }
  await page.getByTestId("ready-planet-chip-jord").tap();
  await page.getByTestId("ready-planet-chip-wellon").tap();
  await shot(page, testInfo, `${prefix}-5a-phone-picked`);
  await page.getByTestId("ready-bar-collapse").tap();
  await page.getByTestId("ready-bar-chips").waitFor({ state: "detached" });
  const box = await page.getByTestId("ready-bar-confirm").boundingBox();
  const viewport = page.viewportSize()!;
  expect(box!.height).toBeGreaterThanOrEqual(44);
  expect(box!.y + box!.height).toBeLessThanOrEqual(viewport.height);
  // A planet on the map is still there above the bar.
  expect(await page.getByTestId("planet-tarmann").boundingBox()).not.toBeNull();
  await shot(page, testInfo, `${prefix}-5b-phone-folded`);
});
