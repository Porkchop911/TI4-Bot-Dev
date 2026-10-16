import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openExtra } from "./extras";
import { prefix } from "./scene";

// A refused history change shows a red toast at the bottom edge. It had no Dismiss and lay over the
// Players / Events buttons until the next attempt; on a phone it now floats above them with a 44 px
// Dismiss. The undo confirmation's buttons are 44 px high too. TI4_SHOT_PREFIX=before is how the
// "before" shots were taken, on the base commit.
test(`${prefix} error toast 390x844`, async ({ page }, testInfo) => {
  await openExtra(page, "history-error", "390x844");
  const toast = (await page.locator(".session-error").boundingBox())!;
  const buttons = (await page.locator(".app-shell__mobile-actions").boundingBox())!;
  const overlap = toast.y + toast.height > buttons.y && toast.y < buttons.y + buttons.height;
  console.log(`MEASURE toast ${Math.round(toast.width)}x${Math.round(toast.height)} at y=${Math.round(toast.y)}; Players/Events at y=${Math.round(buttons.y)}; overlap ${overlap}`);
  await shot(page, testInfo, `${prefix}-error-toast-390x844`);
  if (prefix === "before") return;
  expect(overlap).toBe(false);
  const dismiss = page.getByTestId("history-error-dismiss");
  expect((await dismiss.boundingBox())!.height).toBeGreaterThanOrEqual(43.5);
  await dismiss.click();
  await expect(page.locator(".session-error")).toHaveCount(0);
});

test(`${prefix} undo confirmation 390x844`, async ({ page }, testInfo) => {
  await openExtra(page, "undo-confirm", "390x844");
  const accept = (await page.getByTestId("undo-confirm-accept").boundingBox())!;
  console.log(`MEASURE undo accept ${Math.round(accept.width)}x${Math.round(accept.height)}`);
  await shot(page, testInfo, `${prefix}-undo-confirm-390x844`);
  if (prefix !== "before") expect(accept.height).toBeGreaterThanOrEqual(43.5);
});
