import type { Page, TestInfo } from "@playwright/test";
import { openMockedGame, type MockChoice } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { actor, galleryBoard, hitAssignmentBoard } from "../_shared/fixtures";
import { opponent, playerWithHand } from "../_shared/players";

// TI4_SHOT_PREFIX=before is how the committed "before" shots were taken (on the base commit,
// screenshots-combat-decisions-2026-10-09); the default "after" regenerates the current layout.
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const viewports = [
  { id: "desktop", width: 1440, height: 900 },
  { id: "phone", width: 390, height: 844 },
];

/** System 18: the viewer's mixed fleet (fighters, destroyers, dreadnoughts, carriers) against a rival. */
export function combatBoard(phase: string, round = 1) {
  return {
    ...hitAssignmentBoard,
    combat: {
      system_id: "18",
      round,
      phase,
      attacker: opponent.id,
      defender: actor,
      round_start: hitAssignmentBoard.systems["18"].units,
      attacker_hits: 0,
      defender_hits: 0,
      dice_rolls: [],
    },
  };
}

export async function openCombat(page: Page, viewport: (typeof viewports)[number], phase: string, choice: MockChoice) {
  await page.setViewportSize({ width: viewport.width, height: viewport.height });
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    board: combatBoard(phase) as never,
    choice,
  });
  await page.waitForTimeout(800);
}

export async function take(page: Page, testInfo: TestInfo, id: string, viewport: (typeof viewports)[number]) {
  await shot(page, testInfo, `${prefix}-${id}-${viewport.id}`);
}

export { actor, galleryBoard, opponent, playerWithHand };
