import { test } from "@playwright/test";
import { opponent, openCombat, take, viewports } from "./scene";

// Courageous to the End (a destroy of any ship) and Space Cannon hits bound to non-fighters by
// Graviton Laser System (a normal hit, fighters not valid).
for (const v of viewports) {
  test(`courageous-${v.id}`, async ({ page }, testInfo) => {
    await openCombat(page, v, "resolving_hits", {
      prompt: "assign a hit",
      context: {
        subtype: "courageous_to_the_end_assign_casualty",
        target: { System: "18" },
        space_battle: true,
        hit: { cause: "courageous_to_the_end", destroy: true, restriction: "any", producer: opponent.id },
      },
      options: [
        { id: "destroy|0", label: "destroy fighter", kind: "casualty", payload: { unit: "fighter", damaged: false } },
        { id: "destroy|8", label: "destroy destroyer", kind: "casualty", payload: { unit: "destroyer", damaged: false } },
        { id: "destroy|10", label: "destroy dreadnought", kind: "casualty", payload: { unit: "dreadnought", damaged: false } },
        { id: "destroy|12", label: "destroy carrier", kind: "casualty", payload: { unit: "carrier", damaged: false } },
      ] as never,
    });
    await take(page, testInfo, "courageous", v);
  });

  test(`space-cannon-graviton-${v.id}`, async ({ page }, testInfo) => {
    await openCombat(page, v, "resolving_hits", {
      prompt: "assign a hit",
      context: {
        subtype: "assign_casualty",
        target: { System: "18" },
        space_battle: true,
        outstanding: [{ kind: "UnitsToRemove", amount: 2, paid: 0 }],
        hit: { cause: "space_cannon", destroy: false, restriction: "non_fighter", producer: opponent.id },
      },
      options: [
        { id: "destroy|8", label: "destroy destroyer", kind: "casualty", payload: { unit: "destroyer", damaged: false } },
        { id: "destroy|10", label: "destroy dreadnought", kind: "casualty", payload: { unit: "dreadnought", damaged: false } },
        { id: "destroy|12", label: "destroy carrier", kind: "casualty", payload: { unit: "carrier", damaged: false } },
      ] as never,
    });
    await take(page, testInfo, "space-cannon-graviton", v);
  });
}
