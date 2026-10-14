import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Another seat's hand was shown to you (Mageon Implants, Spy Net): take one card.
test("porkchop decisions: take a revealed card", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("Take a revealed action card"),
  });
  const dialog = page.getByTestId("pending-choice-dialog");
  await dialog.waitFor();
  await shot(page, testInfo, "10-take-revealed", { of: dialog.locator(".choice-dialog__panel"), pad: 8 });
});
