import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { actor, hitAssignmentBoard } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// TI4_SHOT_PREFIX=before is how the committed "before" shots were taken (on the base commit,
// phone-play-2026-10-08); the default "after" regenerates the current layout.
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

const scenarios = [
  { id: "assault-cannon", subtype: "assault_cannon_destroy", prompt: "Assault Cannon: choose a non-fighter ship to destroy" },
  { id: "courageous", subtype: "courageous_to_the_end_assign_casualty", prompt: "Courageous to the End: choose a ship to destroy" },
];
const viewports = [
  { id: "desktop", width: 1440, height: 900 },
  { id: "phone", width: 390, height: 844 },
];

const board = {
  ...hitAssignmentBoard,
  combat: {
    system_id: "18",
    round: 5,
    phase: "resolving_hits",
    attacker: opponent.id,
    defender: actor,
    round_start: hitAssignmentBoard.systems["18"].units,
    attacker_hits: 0,
    defender_hits: 0,
    dice_rolls: [],
  },
};

for (const s of scenarios) {
  for (const v of viewports) {
    test(`${s.id}-${v.id}`, async ({ page }, testInfo) => {
      await page.setViewportSize({ width: v.width, height: v.height });
      await openMockedGame(page, {
        players: [playerWithHand(), opponent],
        board: board as never,
        choice: {
          prompt: s.prompt,
          context: { subtype: s.subtype, target: { System: "18" }, space_battle: true },
          options: [
            { id: "destroy|8", label: "destroy destroyer", kind: "casualty", payload: { unit: "destroyer", damaged: false } },
            { id: "destroy|10", label: "destroy dreadnought", kind: "casualty", payload: { unit: "dreadnought", damaged: false } },
            { id: "destroy|12", label: "destroy carrier", kind: "casualty", payload: { unit: "carrier", damaged: false } },
          ] as never,
        },
      });
      await page.waitForTimeout(800);
      await shot(page, testInfo, `${prefix}-${s.id}-${v.id}`);
    });
  }
}
