import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { confirmBuilds, openProduction, prefix, stageAndConfirm, waitForPanel } from "./scene";

// Interruptions stop the automatic payments: a reaction window is shown as usual and the plan
// resumes after it; a payment that does not match the plan is asked on its own with the
// suggestion staged.
test.describe("interruptions", () => {
  test.skip(prefix === "before", "the plan is new");

  test(`${prefix} reaction window between builds`, async ({ page }, testInfo) => {
    const { ledger } = await openProduction(page, { interruption: "reaction" });
    await stageAndConfirm(page);
    await confirmBuilds(page);
    await waitForPanel(page);
    await page.getByTestId("confirm-payment-btn").click();
    // Build 2 starts, then another decision arrives: nothing is paid, the window is shown.
    await expect(page.getByText("use a card?").first()).toBeVisible();
    await expect.poll(() => ledger.batches.filter((b) => b.kind === "payment").length).toBe(1);
    await shot(page, testInfo, `${prefix}-7-interruption`);
    await page.getByText("Decline", { exact: true }).click();
    await page.getByRole("button", { name: "Confirm choice" }).click();
    // Resumed: the second and third payments are answered from the plan without a panel.
    await expect.poll(() => ledger.batches.filter((b) => b.kind === "payment").length).toBe(3);
    await page.getByTestId("production-builder-drawer").waitFor();
    await expect(page.getByTestId("payment-drawer")).toHaveCount(0);
    await shot(page, testInfo, `${prefix}-8-resumed`);
  });

  test(`${prefix} a payment that does not match the plan`, async ({ page }, testInfo) => {
    const { ledger } = await openProduction(page, { interruption: "mismatch" });
    await stageAndConfirm(page);
    await confirmBuilds(page);
    await waitForPanel(page);
    await page.getByTestId("confirm-payment-btn").click();
    // The engine offers only Quann for build 2, not the planned Lodor: the normal payment is asked.
    await page.getByTestId("production-pay-note").waitFor();
    await page.getByTestId("payment-drawer").waitFor();
    await expect(page.getByTestId("planet-card-exhaust|quann").locator("input")).toBeChecked();
    expect(ledger.batches.filter((b) => b.kind === "payment")).toHaveLength(1);
    await shot(page, testInfo, `${prefix}-9-mismatch`);
  });
});
