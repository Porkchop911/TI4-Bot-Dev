import { test } from "@playwright/test";
import { actor, opponent, openCombat, take, viewports } from "./scene";

// Anti-fighter barrage: the Waylay offer before a side's barrage dice (the only barrage question
// there is: the hits themselves destroy fighters without asking), and the hits once Waylay
// widened them to every ship.
for (const v of viewports) {
  test(`barrage-window-${v.id}`, async ({ page }, testInfo) => {
    await openCombat(page, v, "pre_roll", {
      prompt: "when ANTI_FIGHTER_BARRAGE_STARTED",
      context: {
        subtype: "reaction_when_ANTI_FIGHTER_BARRAGE_STARTED",
        target: { System: "18" },
        space_battle: true,
        trigger: {
          kind: "anti_fighter_barrage",
          event_type: "ANTI_FIGHTER_BARRAGE_STARTED",
          event_id: 7,
          relation: "when",
          actor: opponent.id,
          system: "18",
        },
      },
      options: [
        { id: "play|waylay", label: "Waylay", kind: "reaction", payload: { card: "waylay" } },
        { id: "decline", label: "Pass", kind: "decline" },
      ] as never,
    });
    await take(page, testInfo, "barrage-window", v);
  });

  test(`barrage-waylay-hits-${v.id}`, async ({ page }, testInfo) => {
    await openCombat(page, v, "barrage", {
      prompt: "assign a hit",
      context: {
        subtype: "assign_casualty",
        target: { System: "18" },
        space_battle: true,
        outstanding: [{ kind: "UnitsToRemove", amount: 2, paid: 0 }],
        hit: { cause: "anti_fighter_barrage", destroy: false, restriction: "any", producer: opponent.id },
      },
      options: [
        { id: "destroy|0", label: "destroy fighter", kind: "casualty", payload: { unit: "fighter", damaged: false } },
        { id: "destroy|8", label: "destroy destroyer", kind: "casualty", payload: { unit: "destroyer", damaged: false } },
        { id: "destroy|10", label: "destroy dreadnought", kind: "casualty", payload: { unit: "dreadnought", damaged: false } },
        { id: "destroy|12", label: "destroy carrier", kind: "casualty", payload: { unit: "carrier", damaged: false } },
      ] as never,
    });
    void actor;
    await take(page, testInfo, "barrage-waylay-hits", v);
  });
}
