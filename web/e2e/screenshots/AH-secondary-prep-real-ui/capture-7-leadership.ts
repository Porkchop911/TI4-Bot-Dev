import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting } from "./prep";

// Leadership: the real command token panel with the purchase and its pool arrangement.
test("secondary prep: Leadership purchase", async ({ page }, testInfo) => {
  await openWaiting(page, { name: "Leadership", card: "pok1leadership", viewer: { trade_goods: 3 } });
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("command-token-panel").waitFor();
  await page.getByTestId("token-buy-plus").click();
  await page.getByTestId("token-plus-fleet").click();
  await shot(page, testInfo, "7-leadership");
});
