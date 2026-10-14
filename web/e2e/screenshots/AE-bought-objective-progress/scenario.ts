import type { Page, TestInfo } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent, actor } from "../_shared/players";
import { shot } from "../_shared/shot";
import type { ObjectiveProgressView } from "../../../src/protocol/types";

export type Progress = Record<string, ObjectiveProgressView>;

/** Progress as the server reports it for a bought objective: payable capacity, capped at the cost. */
export const pay = (have: number, threshold = 8): ObjectiveProgressView => ({
  have,
  threshold,
  satisfied: have >= threshold,
});

export const SIZES = {
  desktop: { width: 1440, height: 900 },
  phone: { width: 390, height: 844 },
} as const;

export interface Scenario {
  name: string;
  mine: Progress;
  theirs: Progress;
  scored?: string[];
  /** Open the scoring decision (Status phase) instead of the Objectives button. */
  scoring?: boolean;
}

/** Monument ("Spend 8 resources") and Sway the Council ("Spend 8 influence"), mine and the rival's. */
export async function capture(page: Page, testInfo: TestInfo, size: keyof typeof SIZES, s: Scenario) {
  await page.setViewportSize(SIZES[size]);
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    phase: s.scoring ? "status" : "action",
    table: {
      revealed_objectives: ["monument", "sway_council"],
      scored_objectives: s.scored ? { [actor]: s.scored } : {},
      objective_progress: { [actor]: s.mine, [opponent.id]: s.theirs },
    },
    choice: s.scoring
      ? {
          prompt: "Score an objective",
          context: { subtype: "score_objective" },
          options: [
            { id: "monument", label: "Erect a Monument", kind: "score" },
            { id: "decline", label: "Finish", kind: "decline" },
          ],
        }
      : null,
  });
  if (!s.scoring) await page.getByTestId("objectives-modal-button").click();
  const modal = page.getByTestId("objectives-modal");
  await modal.waitFor();
  // The matrix scrolls sideways on a phone (existing layout); show the player columns.
  await page.evaluate(() => {
    for (const el of document.querySelectorAll<HTMLElement>(".objectives-matrix-container")) {
      el.scrollLeft = el.scrollWidth;
    }
  });
  await shot(page, testInfo, `${s.name}-${size}`, { of: modal, pad: 8 });
}
