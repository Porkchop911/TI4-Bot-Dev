import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting, boardWithExhausted } from "./prep";

// Diplomacy: the real planet selection bar; the exhausted planets glow on the map and a click picks one.
test("secondary prep: Diplomacy, planets on the map", async ({ page }, testInfo) => {
  await openWaiting(page, {
    name: "Diplomacy",
    card: "pok2diplomacy",
    board: boardWithExhausted("exhausted", "wellon"),
  });
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("secondary-yes-btn").click();
  await page.getByTestId("planet-selection-bar").waitFor();
  await page.getByTestId("planet-target-reticle-wellon").waitFor();
  await shot(page, testInfo, "5a-diplomacy-highlighted");
  await page.getByTestId("planet-wellon").click();
  await page.getByTestId("confirm-planet-btn").waitFor();
  await shot(page, testInfo, "5b-diplomacy-picked");
});
