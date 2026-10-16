import type { Page } from "@playwright/test";
import { openMockedGame, type MockChoice } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";

export const SIZES = {
  desktop: { width: 1440, height: 900 },
  "390x844": { width: 390, height: 844 },
} as const;
export type SizeName = keyof typeof SIZES;

/**
 * The viewer holds four secrets: Control the Region (4 of 6 systems), Establish a Perimeter (met,
 * 4 PDS), Hoard Raw Materials (9 of 12 resources) and Prove Endurance (an event secret: no count).
 * Only the viewer's own view carries `secret_objective_progress`; the opponent's has none.
 */
export const hand = playerWithHand({
  secret_objectives_count: 4,
  held_secret_objectives: ["ctr", "eap", "hrm", "pe"],
  secret_objective_progress: {
    ctr: { have: 4, threshold: 6, satisfied: false },
    eap: { have: 4, threshold: 4, satisfied: true },
    hrm: { have: 9, threshold: 12, satisfied: false },
  },
});

export const table = {
  revealed_objectives: ["corner", "lead"],
  scored_objectives: {},
  objective_progress: {
    [hand.id]: {
      corner: { have: 3, threshold: 4, satisfied: false },
      lead: { have: 3, threshold: 3, satisfied: true },
    },
    [opponent.id]: {
      corner: { have: 2, threshold: 4, satisfied: false },
      lead: { have: 1, threshold: 3, satisfied: false },
    },
  },
};

export async function openScene(page: Page, size: SizeName, choice: MockChoice | null = null) {
  await page.setViewportSize(SIZES[size]);
  await openMockedGame(page, {
    phase: choice ? "status" : "action",
    board: galleryBoard,
    players: [hand, opponent],
    table,
    choice:
      choice ??
      ({
        prompt: "action phase",
        player: opponent.id,
        options: [{ id: "pass", label: "pass", kind: "action" }],
      } as MockChoice),
  });
  await page.getByTestId("ti4-board-svg").waitFor();
  await page.waitForTimeout(400);
}
