import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openDraft } from "./_draft";

// The Thunder's Edge draft on a phone (390 px).
test("strategy card set: draft on a phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openDraft(page, "te");
  await expect(page.getByText(/You may redistribute your command tokens before and after/)).toBeVisible();
  await shot(page, testInfo, "9-draft-te-phone");
});
