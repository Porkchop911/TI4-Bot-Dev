import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { chooseThePayment, openScene, prefix, rejectingScript } from "./scene";

// The engine checks the whole purchase when it is saved. If its own flow would not offer a step of
// the payment, nothing is saved and the panel says which. After only: before there was no check.
test.describe("leadership prep: variants", () => {
  test.skip(prefix === "before", "the check only exists with the exact payment");

  test("the engine does not offer the chosen payment", async ({ page }, testInfo) => {
    await openScene(page, rejectingScript);
    await chooseThePayment(page);
    await page.getByTestId("token-bar-confirm").click();
    await page.getByTestId("token-payment-bar").getByRole("alert").waitFor();
    await shot(page, testInfo, "after-5-engine-rejects-the-payment");
  });
});
