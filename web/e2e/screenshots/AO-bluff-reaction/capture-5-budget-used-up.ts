import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { intentState, openBluffGame, openSelector, ownCard } from "./_bluff";

// The stall budget is spent: further bluff waits are skipped silently, and only this seat is told.
test("selector: bluff budget used up", async ({ page }, testInfo) => {
  const { send } = await openBluffGame(page);
  send(intentState({ triggers: ["agenda"], locked_until_round: 3, budget_used_up: true }));
  const selector = await openSelector(page);
  await expect(page.getByTestId("bluff-budget-used-up")).toBeVisible();
  await expect(selector).toHaveAttribute("data-state", "locked");
  await shot(page, testInfo, "5-budget-used-up", { of: ownCard(page), pad: 8 });
});
