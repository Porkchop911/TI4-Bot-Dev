import { test } from "@playwright/test";
import { bombardmentTargetChoice, invasionBoard, openInvasion, take, viewports, withBombardTargets } from "./scene";

// The coexistence bombardment question: several players have ground forces on the planet.
for (const v of viewports) {
  test(`bombardment-decision-${v.id}`, async ({ page }, testInfo) => {
    const board = invasionBoard({ phase: "bombardment", units: withBombardTargets });
    await openInvasion(page, v, board, bombardmentTargetChoice);
    await take(page, testInfo, "bombardment-decision", v);
  });
}
