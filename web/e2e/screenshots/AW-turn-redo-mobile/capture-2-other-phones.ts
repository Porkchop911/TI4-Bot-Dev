import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { expectLayout } from "./layout";
import { measure, openScene, phones, prefix, scenes } from "./scenes";

// The same states at 412x915 and in landscape (844x390): measured for the before/after table
// (metrics/*.json), asserted after the change, and pictured for the landscape conflict only.
const others = phones.filter((vp) => vp.id === "412x915" || vp.id === "844x390");

for (const vp of others) {
  for (const scene of scenes) {
    test(`${prefix} ${vp.id} ${scene.id}`, async ({ page }, testInfo) => {
      await page.setViewportSize({ width: vp.width, height: vp.height });
      await openScene(page, scene);
      await measure(page, testInfo, `${vp.id} ${scene.id}`);
      if (vp.id === "844x390" && ["7-conflict", "9-with-decision"].includes(scene.id)) await shot(page, testInfo, `${prefix}-${vp.id}-${scene.id}`);
      if (prefix === "after" && !scene.log) await expectLayout(page, scene, vp);
    });
  }
}
