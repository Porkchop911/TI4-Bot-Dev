import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";
import { shot } from "../_shared/shot";

/** `before` is taken on the commit without the change. */
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";
const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;

// Status phase scoring: one public objective and one secret objective are in reach (61.6); the
// seat also holds two secrets it cannot score yet.
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`status scoring with a secret ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await openMockedGame(page, {
      phase: "status",
      board: galleryBoard,
      players: [
        playerWithHand({ secret_objectives_count: 3, held_secret_objectives: ["ans", "ctr", "baf"] }),
        opponent,
      ],
      table: { revealed_objectives: ["corner", "lead"], scored_objectives: {} },
      choice: {
        prompt: "score an objective",
        context: { subtype: "score_objective", source: { Rule: "61.6" } },
        options: [
          { id: "corner", label: "corner", kind: "score" },
          { id: "ans", label: "ans", kind: "score" },
          { id: "decline", label: "decline", kind: "decline" },
        ],
      },
    });
    await page.getByTestId("objectives-modal").waitFor();
    await page.waitForTimeout(700);
    if (prefix === "after") {
      const row = page.getByTestId("secret-row-ans");
      await expect(row).toContainText("Adapt New Strategies");
      await expect(row).toContainText("Own 2 faction technologies");
      await expect(page.getByTestId("secret-row-held-ctr")).toContainText("not scoreable now");
      await row.locator('[data-testid="choice-option"]').click();
      await expect(page.getByTestId("objectives-scoring-footer")).toContainText(
        "Adapt New Strategies (+1 VP)",
      );
      await row.scrollIntoViewIfNeeded();
    }
    await shot(page, testInfo, `${prefix}-status-${size}`);
  });
}

// A secret scored outside the status phase (action-phase timing): no public objective revealed
// that the seat can score, the secret alone.
test("action phase secret 1440x900", async ({ page }, testInfo) => {
  await page.setViewportSize(SIZES.desktop);
  await openMockedGame(page, {
    board: galleryBoard,
    players: [playerWithHand({ secret_objectives_count: 1, held_secret_objectives: ["baf"] }), opponent],
    table: { revealed_objectives: [], scored_objectives: {} },
    choice: {
      prompt: "score an objective",
      context: { subtype: "score_secret_objective", source: { Rule: "61.6" } },
      options: [
        { id: "baf", label: "baf", kind: "score" },
        { id: "decline", label: "decline", kind: "decline" },
      ],
    },
  });
  await page.getByTestId("objectives-modal").waitFor();
  await page.waitForTimeout(700);
  if (prefix === "after") {
    await expect(page.getByTestId("secret-row-baf")).toContainText("Betray a Friend");
    await expect(page.getByTestId("objectives-modal")).toContainText("Score Secret Objective");
  }
  await shot(page, testInfo, `${prefix}-action-secret-desktop`);
});
