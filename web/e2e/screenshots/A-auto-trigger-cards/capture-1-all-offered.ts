import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryPlayers } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Default state: every held action card is offered normally (green tick).
test("action cards: all offered", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent ?? galleryPlayers[1]] });
  const card = page.getByTestId("player-card").first();
  await card.waitFor();
  await shot(page, testInfo, "1-all-offered", { of: card, pad: 6 });
});
