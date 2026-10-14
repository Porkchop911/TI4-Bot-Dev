import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting } from "./prep";

// Phone width: the banner and the real panel on a 390 px screen.
test.use({ viewport: { width: 390, height: 844 } });
test("secondary prep: phone", async ({ page }, testInfo) => {
  await openWaiting(page, { name: "Politics", card: "pok3politics" });
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("strategy-secondary-panel").waitFor();
  await shot(page, testInfo, "13-phone");
});
