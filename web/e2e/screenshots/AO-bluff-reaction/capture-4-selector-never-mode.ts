import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openBluffGame, openSelector, ownCard } from "./_bluff";

// A seat that set a card to Never offer wants a faster game, so it cannot also stall one.
test("selector: disabled in Never mode", async ({ page }, testInfo) => {
  await openBluffGame(page);
  // The real toggle: the mocked server answers with the seat's modes, as the real one does.
  await page.getByTestId("reaction-inspect-mode-sabo1").click();
  const selector = await openSelector(page);
  await expect(selector).toHaveAttribute("data-state", "disabled");
  await expect(page.getByTestId("bluff-disabled-reason")).toContainText("Never offer");
  await shot(page, testInfo, "4-selector-never-mode", { of: ownCard(page), pad: 8 });
});
