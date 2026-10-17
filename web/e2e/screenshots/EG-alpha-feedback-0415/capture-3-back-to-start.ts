import { expect, test } from "@playwright/test";
import { openMapLobby } from "../_shared/mapLobby";
import { shot } from "../_shared/shot";
import { prefix } from "./scene";

// fb_1791875585: no way from a lobby page back to the start screen (create a lobby, My games).
test("lobby back to start", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openMapLobby(page, { players: 4, viewer: "guest_1" });
  await page.getByTestId("lobby-container").waitFor();
  const button = page.getByTestId("lobby-back-to-start");
  if (prefix === "after") {
    await button.scrollIntoViewIfNeeded();
    await expect(button).toBeVisible();
  } else {
    await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
  }
  await shot(page, testInfo, `${prefix}-lobby-back-to-start-390x844`);
});
