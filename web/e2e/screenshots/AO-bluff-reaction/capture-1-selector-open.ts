import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openBluffGame, openSelector, ownCard } from "./_bluff";

// A seat with action cards and no Never setting: every moment can be picked.
test("selector on the player mat: open, nothing declared", async ({ page }, testInfo) => {
  await openBluffGame(page);
  await openSelector(page);
  await shot(page, testInfo, "1-selector-open", { of: ownCard(page), pad: 8 });
});
