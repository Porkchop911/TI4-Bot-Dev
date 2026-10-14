import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting } from "./prep";

// Preparation mode: the REAL strategy secondary panel, fed by a client-made stand-in question.
test("secondary prep: follow or skip with the real panel", async ({ page }, testInfo) => {
  await openWaiting(page, { name: "Politics", card: "pok3politics" });
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("strategy-secondary-panel").waitFor();
  await shot(page, testInfo, "2-follow-skip");
});
