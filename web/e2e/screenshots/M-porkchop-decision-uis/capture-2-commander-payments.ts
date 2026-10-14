import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// The Crimson and Deepwrought commanders pay their holder: gain 1 commodity or convert 1.
for (const [name, title] of [
  ["2-crimson-pay", "Crimson commander: gain or convert"],
  ["3-deepwrought-pay", "Deepwrought commander: gain or convert"],
  ["4-deepwrought-reduce", "Deepwrought commander: reduce research"],
  ["5-research-waiver", "Research waiver: ignore prerequisites"],
  ["6-waiver-pay", "Research waiver: choose the payment"],
  ["8-pds-alternative", "Construction: PDS or an alternative"],
  ["9-reinforce-place", "Reinforcements: choose where to place"],
  ["11-coexist", "Slumberstate Computing: coexist or fight"],
] as const) {
  test(`porkchop decisions: ${name}`, async ({ page }, testInfo) => {
    await openMockedGame(page, {
      players: [playerWithHand({ trade_goods: 4 }), opponent],
      choice: galleryDecision(title),
    });
    await page.getByTestId("offer-card-panel").waitFor();
    await shot(page, testInfo, name, { of: page.getByTestId("pending-choice-dialog").locator(".choice-dialog__panel"), pad: 8 });
  });
}
