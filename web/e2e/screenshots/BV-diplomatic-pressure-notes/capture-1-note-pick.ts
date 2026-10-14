import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";
import { shot } from "../_shared/shot";

/** `before` is taken on the commit without the fix. */
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";
const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;

// Diplomatic Pressure: the chosen player picks which promissory note to give.
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`diplomatic pressure note pick ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await openMockedGame(page, {
      board: galleryBoard,
      players: [playerWithHand({}), opponent],
      choice: {
        prompt: "Diplomatic Pressure: which note do you give",
        context: { subtype: "dp1_pick_promissory note", source: { ActionCard: "dp1" } },
        options: [
          { id: "cf:Xxcha Kingdom", label: "cf:Xxcha Kingdom", kind: "promissory note" },
          { id: "ps:Federation of Sol", label: "ps:Federation of Sol", kind: "promissory note" },
          { id: "support:Hacan", label: "support:Hacan", kind: "promissory note" },
        ],
      },
    });
    await page.getByTestId("choice-option").first().waitFor();
    await page.waitForTimeout(700);
    if (prefix === "after") {
      await expect(page.getByTestId("card-option-text").first()).toContainText("cannot move units");
    }
    await shot(page, testInfo, `${prefix}-${size}`);
  });
}
