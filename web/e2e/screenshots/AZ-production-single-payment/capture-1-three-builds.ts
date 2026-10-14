import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { confirmBuilds, openProduction, prefix, stageAndConfirm, waitForPanel } from "./scene";

// Three cruisers (2 resources each) in Mecatol Rex. Before: the payment is asked for each build.
// After: one panel for the whole production, then the later payments follow from it.
// TI4_SHOT_PREFIX=before turns the new behaviour off ("Ask for each payment"), which is the flow
// of the base commit.
test(`${prefix} three builds, desktop`, async ({ page }, testInfo) => {
  const { ledger } = await openProduction(page);
  await stageAndConfirm(page);
  await shot(page, testInfo, `${prefix}-1-staged`);
  await confirmBuilds(page);

  let prompts = 0;
  if (prefix === "before") {
    for (let i = 1; i <= 3; i++) {
      await page.getByTestId("payment-drawer").waitFor();
      await expect(page.getByTestId("payment-drawer-title")).toContainText("Pay");
      await page.getByTestId("auto-pay-btn").click();
      await expect(page.getByTestId("confirm-payment-btn")).toBeEnabled();
      await shot(page, testInfo, `${prefix}-2-prompt-${i}`);
      prompts += 1;
      await page.getByTestId("confirm-payment-btn").click();
    }
  } else {
    await waitForPanel(page);
    await expect(page.getByTestId("payment-drawer-title")).toContainText("Pay for 3 units: total");
    await expect(page.getByTestId("committed-amount")).toContainText("7");
    await shot(page, testInfo, `${prefix}-2-panel`);
    prompts += 1;
    await page.getByTestId("confirm-payment-btn").click();
    // The later payments are answered from the plan: no payment panel appears again.
    await page.getByTestId("production-builder-drawer").waitFor();
    await expect(page.getByTestId("payment-drawer")).toHaveCount(0);
  }
  await expect.poll(() => ledger.batches.filter((b) => b.kind === "payment").length).toBe(3);
  await page.getByTestId("production-builder-drawer").waitFor();
  await expect(page.getByTestId("production-capacity-counter")).toContainText("3 / 6");
  expect(prompts).toBe(prefix === "before" ? 3 : 1);
  expect(ledger.payQuestions).toBe(3);
  if (prefix !== "before")
    expect(ledger.batches.filter((b) => b.kind === "payment").map((b) => b.steps)).toEqual([
      [{ kind: "exhaust", planet: "jord" }],
      [{ kind: "exhaust", planet: "lodor" }],
      [{ kind: "exhaust", planet: "quann" }],
    ]);
  await shot(page, testInfo, `${prefix}-3-result`);
  await page.getByTestId("close-production-drawer").click();
  await shot(page, testInfo, `${prefix}-4-board`);
});
