import { test } from "@playwright/test";
import { capture, viewports, lieInWait, seizeFragment } from "./scene";

const scenes = { lieInWait, seizeFragment };

for (const [name, scene] of Object.entries(scenes)) {
  for (const viewport of viewports) {
    test(`${name}-${viewport.id}`, async ({ page }, testInfo) => {
      await capture(page, testInfo, name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`), viewport, scene);
    });
  }
}
