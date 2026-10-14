import { test } from "@playwright/test";
import { hoverShot } from "../_shared/hover";
import { openMockedLobby } from "../_shared/mockLobby";

// In the lobby the host can remove a player or bot from a seat; the button's label is a native title.
test("tooltip: lobby remove button", async ({ page }, testInfo) => {
  await openMockedLobby(page);
  const button = page.getByTestId("remove-bot-button-2");
  await button.waitFor();
  await hoverShot(page, testInfo, "28-lobby-remove-button", button, { native: true, pad: 90 });
});
