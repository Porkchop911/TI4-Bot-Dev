import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { actionCardEventLog } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { routeTurnRedoStatus, reservedCard } from "./redoMock";

// A card the original timeline handed a seat is no longer in the deck: that draw cannot give the
// seat the same card, so the replay stops there and the seat decides live.
test("turn redo: reserved card gone", async ({ page }, testInfo) => {
  await routeTurnRedoStatus(page, reservedCard);
  await openMockedGame(page, { players: [playerWithHand(), opponent], events: actionCardEventLog(), phase: "action" });
  const bar = page.getByTestId("turn-redo-bar");
  await expect(bar).toContainText("is no longer in the deck");
  await expect(bar).not.toContainText("different number of cards");
  await shot(page, testInfo, "8-reserved-card");
});
