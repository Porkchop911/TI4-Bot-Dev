import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { playerWithHand, opponent } from "../_shared/players";

// Each map overlay explains what it shows.
test("tooltip: map overlay button", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent] });
  await hoverShot(page, testInfo, "03-overlay-toolbar", page.getByRole("button", { name: /Ground Combat/ }));
});
