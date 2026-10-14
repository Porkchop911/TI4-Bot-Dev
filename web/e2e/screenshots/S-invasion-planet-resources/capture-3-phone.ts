import { test } from "@playwright/test";
import { openLanding, capture } from "./fixture";

test("3-phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openLanding(page, [
    { id: "jord", label: "Jord", resources: 2, influence: 2, traits: ["cultural"] },
    { id: "exhausted", label: "Exhausted World", resources: 1, influence: 3, traits: ["industrial"], attachments: ["dmz"] },
  ]);
  await capture(page, testInfo, "3-phone");
});
