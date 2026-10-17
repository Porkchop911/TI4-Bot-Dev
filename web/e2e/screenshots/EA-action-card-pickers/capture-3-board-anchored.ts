import { test } from "@playwright/test";
import { capture, viewports, upgrade, plague, rescue } from "./scene";

const scenes = { upgrade, plague, rescue };

for (const [name, scene] of Object.entries(scenes)) {
  for (const viewport of viewports) {
    test(`${name}-${viewport.id}`, async ({ page }, testInfo) => {
      await capture(page, testInfo, name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`), viewport, scene);
    });
  }
}
