import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { playerWithHand, opponent } from "../_shared/players";

// The turn sound button says what a click does.
test("tooltip: turn sound toggle", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent] });
  await hoverShot(page, testInfo, "13-sound-button", page.getByRole("button", { name: /Sound/ }).first(), { native: true });
});
