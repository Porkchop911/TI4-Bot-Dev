import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openDraft } from "./_draft";

// The draft in a Thunder's Edge game: Construction and Warfare show their Thunder's Edge text.
test("strategy card set: draft, Thunder's Edge", async ({ page }, testInfo) => {
  await openDraft(page, "te");
  await expect(page.getByText(/Either place 1 structure on a planet you control/)).toBeVisible();
  await expect(page.getByText(/You may redistribute your command tokens before and after/)).toBeVisible();
  await shot(page, testInfo, "7-draft-te");
});
