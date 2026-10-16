import { expect, test } from "@playwright/test";
import { openCombat, take, viewports } from "../BK-hit-kind-clarity/scene";
import { opponent } from "../_shared/players";

// The space combat dialog names the system, not only its number.
for (const v of viewports) {
  test(`combat title ${v.id}`, async ({ page }, testInfo) => {
    await openCombat(page, v, "resolving_hits", {
      prompt: "assign a hit",
      context: {
        subtype: "assault_cannon_destroy",
        target: { System: "18" },
        space_battle: true,
        hit: { cause: "assault_cannon", destroy: true, restriction: "non_fighter", producer: opponent.id },
      },
      options: [
        { id: "destroy|10", label: "destroy dreadnought", kind: "casualty", payload: { unit: "dreadnought", damaged: false } },
      ] as never,
    });
    await expect(page.getByTestId("combat-stage-title")).toContainText("Mecatol Rex (System 18)");
    await take(page, testInfo, "combat-title", v);
  });
}
