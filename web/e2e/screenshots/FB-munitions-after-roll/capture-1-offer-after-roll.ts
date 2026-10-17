import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { afterRollOffer } from "./scene";

const panel = (page: import("@playwright/test").Page) =>
  page.getByTestId("pending-choice-dialog").locator(".choice-dialog__panel");

// Munitions Reserves is asked after the fleet has rolled: the dice are on the card.
test("munitions: offer after the roll", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand({ trade_goods: 5 }), opponent],
    choice: afterRollOffer() as never,
  });
  await page.getByTestId("ability-offer-panel").waitFor();
  await shot(page, testInfo, "1-offer-after-roll", { of: panel(page), pad: 8 });
});

// A single miss: only "reroll the missed die", no choosing.
test("munitions: offer with a single miss", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand({ trade_goods: 5 }), opponent],
    choice: afterRollOffer(
      [
        { unit: "cruiser", unit_index: 0, die: 0, face: 8, bonus: 0, hits_on: 7, rerolled: false },
        { unit: "cruiser", unit_index: 0, die: 1, face: 2, bonus: 0, hits_on: 7, rerolled: false },
      ],
      1,
    ) as never,
  });
  await page.getByTestId("ability-offer-panel").waitFor();
  await shot(page, testInfo, "2-offer-single-miss", { of: panel(page), pad: 8 });
});
