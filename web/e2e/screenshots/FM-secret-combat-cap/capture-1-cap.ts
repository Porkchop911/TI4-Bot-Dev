import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";
import { shot } from "../_shared/shot";

const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;

type Variant = { name: string; held: string[]; offered: string[]; pick?: string; note: boolean };
const VARIANTS: Variant[] = [
  { name: "one-qualifying", held: ["btv", "ans"], offered: ["btv"], note: true },
  { name: "two-qualifying", held: ["btv", "dyp"], offered: ["btv", "dyp"], pick: "dyp", note: true },
  { name: "uncapped-mew", held: ["mew", "ans"], offered: ["mew"], note: false },
];

// Action-phase scoring after an event: the cap note only shows for secrets whose card prints
// "Only 1 objective can be scored during or after each combat."
for (const v of VARIANTS) {
  for (const [size, viewport] of Object.entries(SIZES)) {
    test(`${v.name} ${size}`, async ({ page }, testInfo) => {
      await page.setViewportSize(viewport);
      await openMockedGame(page, {
        board: galleryBoard,
        players: [
          playerWithHand({ secret_objectives_count: v.held.length, held_secret_objectives: v.held }),
          opponent,
        ],
        table: { revealed_objectives: [], scored_objectives: {} },
        choice: {
          prompt: "score a secret objective",
          context: { subtype: "score_secret_objective", source: { Rule: "61.6" } },
          options: [
            ...v.offered.map((id) => ({ id, label: id, kind: "score" })),
            { id: "decline", label: "decline", kind: "decline" },
          ],
        },
      });
      await page.getByTestId("objectives-modal").waitFor();
      await page.waitForTimeout(700);
      const note = page.getByTestId("secret-combat-cap-note");
      if (v.note) {
        await expect(note).toContainText("Only 1 objective can be scored during or after each combat.");
      } else {
        await expect(note).toHaveCount(0);
      }
      if (v.pick) {
        await page
          .locator(`[data-testid="choice-option"][data-option-id="${v.pick}"]`)
          .click();
        await expect(page.getByTestId("secret-skipped-btv")).toBeVisible();
        await expect(note).toContainText("closes the window for the other");
      }
      if (size === "390x844") {
        const overflow = await page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        );
        expect(overflow).toBeLessThanOrEqual(0);
      }
      await shot(page, testInfo, `${v.name}-${size}`);
    });
  }
}

// The held-secret line (player sheet / objectives dialog outside a window) carries the cap too.
test("held secret note 1440x900", async ({ page }, testInfo) => {
  await page.setViewportSize(SIZES.desktop);
  await openMockedGame(page, {
    phase: "action",
    board: galleryBoard,
    players: [playerWithHand({ secret_objectives_count: 2, held_secret_objectives: ["btv", "mew"] }), opponent],
    table: { revealed_objectives: [], scored_objectives: {}, objective_progress: {} },
    choice: { prompt: "action phase", player: opponent.id, options: [{ id: "pass", label: "pass", kind: "action" }] },
  });
  await page.getByTestId("ti4-board-svg").waitFor();
  await page.waitForTimeout(400);
  await expect(page.getByTestId("secret-progress-note-btv")).toContainText("Only 1 objective");
  await expect(page.getByTestId("secret-progress-note-mew")).not.toContainText("Only 1");
  await shot(page, testInfo, "held-note-desktop");
});
