import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { expectLayout } from "./layout";
import { measure, openScene, prefix, scenes } from "./scenes";

// Every state of the turn redo UI on a phone, before and after. TI4_SHOT_PREFIX=before is how the
// committed "before" shots and metrics were taken (on the base commit, unified-latest-2026-10-08);
// the default "after" regenerates the current layout and asserts, at 390x844 and 360x740, that the
// collapsed strip is at most 48 px tall, the open sheet at most 40% of the viewport height, and that
// neither covers the header, the toolbar and seat legend, the Players / Events buttons, the turn
// action bar or the decision dialog's primary buttons (see layout.ts).
const viewports = [
  { id: "390x844", width: 390, height: 844, scenes: scenes.map((s) => s.id) },
  { id: "360x740", width: 360, height: 740, scenes: ["4-replaying", "5-handoff", "7-conflict", "10-with-turn-bar"] },
];

for (const vp of viewports) {
  for (const scene of scenes.filter((s) => vp.scenes.includes(s.id))) {
    test(`${prefix} ${vp.id} ${scene.id}`, async ({ page }, testInfo) => {
      await page.setViewportSize({ width: vp.width, height: vp.height });
      await openScene(page, scene);
      await measure(page, testInfo, `${vp.id} ${scene.id}`);
      await shot(page, testInfo, `${prefix}-${vp.id}-${scene.id}`);
      if (prefix === "after" && !scene.log) await expectLayout(page, scene, vp);
    });
  }
}

// After only: the pill opened by hand while the replay runs or the new turn is played.
for (const vp of viewports.slice(0, 1)) {
  for (const id of ["3-new-turn", "4-replaying"]) {
    test(`after ${vp.id} ${id} opened`, async ({ page }, testInfo) => {
      test.skip(prefix !== "after");
      await page.setViewportSize({ width: vp.width, height: vp.height });
      const scene = scenes.find((s) => s.id === id)!;
      await openScene(page, scene);
      await page.getByTestId("turn-redo-toggle").click();
      await expect(page.getByTestId("turn-redo-sheet")).toBeVisible();
      await expectLayout(page, scene, vp);
      await shot(page, testInfo, `after-${vp.id}-${id}-opened`);
    });
  }
}
