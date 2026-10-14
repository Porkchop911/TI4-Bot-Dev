import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openScene, prefix } from "./scene";

// The two variants of the same panel that are not the exact case. Both are captured with the new
// code only: a seat the engine would not ask right now (the plan is still allowed, and says so),
// and an older server that does not know the preview message (the previous estimate, flagged
// approximate, and no error shown to the player).
test.describe("secondary prep: variants", () => {
  test.skip(prefix === "before", "variants only exist with the preview");

  test("the engine would not ask you as of now", async ({ page }, testInfo) => {
    await openScene(page, {
      name: "Technology",
      card: "pok7technology",
      viewer: { trade_goods: 0 },
      script: () => ({ status: "would_not_be_asked", blocker: "cannot_pay_resources" }),
    });
    await page.getByTestId("secondary-prep-chip").click();
    await page.getByTestId("prepare-not-asked").waitFor();
    await shot(page, testInfo, "after-4a-not-asked-as-of-now");
  });

  test("an older server falls back to the estimate", async ({ page }, testInfo) => {
    await openScene(page, { name: "Technology", card: "pok7technology", script: null });
    await page.getByTestId("secondary-prep-chip").click();
    await page.getByTestId("secondary-yes-btn").click();
    await page.getByTestId("prepare-approximate").waitFor();
    await shot(page, testInfo, "after-4b-older-server-estimate");
  });
});
