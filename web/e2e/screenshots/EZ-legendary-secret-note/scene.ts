import type { Page } from "@playwright/test";
import { openMockedGame, type MockChoice } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";

export const SIZES = {
  desktop: { width: 1440, height: 900 },
  "390x844": { width: 390, height: 844 },
} as const;
export type SizeName = keyof typeof SIZES;

/** The viewer holds Seize an Icon (control a legendary planet) and Control the Region. */
export const hand = playerWithHand({
  secret_objectives_count: 2,
  held_secret_objectives: ["sai", "ctr"],
  secret_objective_progress: {
    sai: { have: 0, threshold: 1, satisfied: false },
    ctr: { have: 2, threshold: 6, satisfied: false },
  },
});

/** The gallery board has legendary planets; this copy is the map of the reported game without any. */
export const boardWithoutLegendary = {
  ...galleryBoard,
  map_tiles: (galleryBoard.map_tiles ?? []).map((tile) => ({
    ...tile,
    planets: (tile.planets ?? []).map((planet) => ({ ...planet, legendary: false })),
  })),
};

export async function openScene(page: Page, size: SizeName, withLegendary: boolean) {
  await page.setViewportSize(SIZES[size]);
  await openMockedGame(page, {
    phase: "action",
    board: withLegendary ? galleryBoard : boardWithoutLegendary,
    players: [hand, opponent],
    choice: {
      prompt: "action phase",
      player: opponent.id,
      options: [{ id: "pass", label: "pass", kind: "action" }],
    } as MockChoice,
  });
  await page.getByTestId("ti4-board-svg").waitFor();
  await page.waitForTimeout(400);
}
