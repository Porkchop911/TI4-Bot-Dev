import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting } from "./prep";

// Another seat is resolving Technology; the follower sees the status bar and a small affordance.
test("secondary prep: waiting, the prepare chip", async ({ page }, testInfo) => {
  await openWaiting(page, { name: "Technology", card: "pok7technology" });
  await page.getByTestId("secondary-prep-chip").waitFor();
  await shot(page, testInfo, "1-waiting-chip");
});
