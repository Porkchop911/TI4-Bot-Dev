import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openBluffGame, openSelector, ownCard } from "./_bluff";

// Three moments declared. The server accepts the set and locks it until the next round; the other
// moments are off because three is the limit.
test("selector: three declared, locked until the next round", async ({ page }, testInfo) => {
  await openBluffGame(page);
  const selector = await openSelector(page);
  for (const id of ["system_activated", "space_combat", "agenda"])
    await page.getByTestId(`bluff-trigger-${id}`).check();
  await page.getByTestId("bluff-declare").click();
  await expect(selector).toHaveAttribute("data-state", "locked");
  await expect(page.getByTestId("bluff-locked-reason")).toContainText("round 3");
  await shot(page, testInfo, "2-selector-declared", { of: ownCard(page), pad: 8 });
});
