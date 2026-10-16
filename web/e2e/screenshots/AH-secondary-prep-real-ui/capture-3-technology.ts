import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting } from "./prep";

// Technology: follow, then the real technology picker with its cost; approximate until the real question opens.
test("secondary prep: Technology picker", async ({ page }, testInfo) => {
  await openWaiting(page, { name: "Technology", card: "pok7technology", viewer: { trade_goods: 5 } });
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("secondary-yes-btn").click();
  await page.getByTestId("technology-modal").waitFor();
  await page.locator('[data-testid^="tech-card-"][data-selectable="true"]').first().click();
  await shot(page, testInfo, "3-technology");
});
