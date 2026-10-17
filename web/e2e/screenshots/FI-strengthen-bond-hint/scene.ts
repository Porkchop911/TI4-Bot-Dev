import type { Page } from "@playwright/test";
import { openMockedGame, type MockChoice } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";

export const SIZES = {
  desktop: { width: 1440, height: 900 },
  "390x844": { width: 390, height: 844 },
} as const;
export type SizeName = keyof typeof SIZES;

/** The viewer holds Strengthen Bonds with 0 / 1: a rival's note sits in the hand, not the play area. */
export const hand = playerWithHand({
  secret_objectives_count: 1,
  held_secret_objectives: ["sb"],
  secret_objective_progress: { sb: { have: 0, threshold: 1, satisfied: false } },
});

export async function openScene(page: Page, size: SizeName) {
  await page.setViewportSize(SIZES[size]);
  await openMockedGame(page, {
    phase: "action",
    board: galleryBoard,
    players: [hand, opponent],
    table: { revealed_objectives: [], scored_objectives: {}, objective_progress: {} },
    choice: {
      prompt: "action phase",
      player: opponent.id,
      options: [{ id: "pass", label: "pass", kind: "action" }],
    } as MockChoice,
  });
  await page.getByTestId("ti4-board-svg").waitFor();
  await page.waitForTimeout(400);
}
