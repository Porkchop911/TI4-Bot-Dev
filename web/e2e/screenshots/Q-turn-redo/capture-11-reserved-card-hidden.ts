import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { actionCardEventLog } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { routeTurnRedoStatus, reservedHiddenCard } from "./redoMock";

// The same conflict for a card of a hidden deck (action card, secret objective): only the seat it
// was reserved for is told which card; everyone else is told that one of that deck is missing.
test("turn redo: reserved card gone, hidden deck", async ({ page }, testInfo) => {
  await routeTurnRedoStatus(page, reservedHiddenCard);
  await openMockedGame(page, { players: [playerWithHand(), opponent], events: actionCardEventLog(), phase: "action" });
  const bar = page.getByTestId("turn-redo-bar");
  await expect(bar).toContainText("an action card reserved for");
  await shot(page, testInfo, "11-reserved-card-hidden");
});
