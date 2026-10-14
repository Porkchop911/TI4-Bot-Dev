import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openDraft } from "./_draft";

// The same draft in a Prophecy of Kings game: the PoK Construction and Warfare text.
test("strategy card set: draft, Prophecy of Kings", async ({ page }, testInfo) => {
  await openDraft(page, "pok");
  await expect(page.getByText(/Remove 1 of your command tokens from the game board/)).toBeVisible();
  await expect(page.getByText(/Either place 1 structure on a planet you control/)).toHaveCount(0);
  await shot(page, testInfo, "8-draft-pok");
});
