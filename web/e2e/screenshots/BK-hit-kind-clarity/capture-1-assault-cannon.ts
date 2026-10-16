import { test } from "@playwright/test";
import { actor, opponent, openCombat, take, viewports } from "./scene";

// Assault Cannon: the opponent's cannon destroys one of the viewer's non-fighter ships; the viewer
// (the victim) picks which. The context carries the engine's `hit` description.
for (const v of viewports) {
  test(`assault-cannon-${v.id}`, async ({ page }, testInfo) => {
    await openCombat(page, v, "resolving_hits", {
      prompt: "assign a hit",
      context: {
        subtype: "assault_cannon_destroy",
        target: { System: "18" },
        space_battle: true,
        hit: { cause: "assault_cannon", destroy: true, restriction: "non_fighter", producer: opponent.id },
      },
      options: [
        { id: "destroy|8", label: "destroy destroyer", kind: "casualty", payload: { unit: "destroyer", damaged: false } },
        { id: "destroy|10", label: "destroy dreadnought", kind: "casualty", payload: { unit: "dreadnought", damaged: false } },
        { id: "destroy|12", label: "destroy carrier", kind: "casualty", payload: { unit: "carrier", damaged: false } },
      ] as never,
    });
    void actor;
    await take(page, testInfo, "assault-cannon", v);
  });
}
