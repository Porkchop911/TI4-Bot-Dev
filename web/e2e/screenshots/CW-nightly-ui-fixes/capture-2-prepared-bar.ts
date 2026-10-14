import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting, push, save, secondaryQuestion } from "../AH-secondary-prep-real-ui/prep";

// The "Prepared ... Confirm / Choose myself" bar sat at the top of the board column and covered the
// head of the token panel (the "Pay with planets" header). It now docks at the bottom.
async function scene(page: import("@playwright/test").Page, variant: "before" | "after") {
  const game = await openWaiting(page, { name: "Technology", card: "pok7technology" });
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("secondary-yes-btn").click();
  await save(page);
  if (variant === "before")
    await page.addStyleTag({
      content: ".secondary-prep { top: calc(var(--board-chrome-bottom, 56px) + 8px) !important; bottom: auto !important; }",
    });
  push(game, {
    version: 42,
    history: { cursor: 41, redo_count: 0, generation: 0 },
    choice: secondaryQuestion("pok7technology", "spend a strategy token and 4 resources to research", "spend"),
  });
  await page.getByTestId("secondary-prepared-confirm").waitFor();
  await page.waitForTimeout(300);
}

for (const variant of ["before", "after"] as const) {
  test(`prepared bar desktop ${variant}`, async ({ page }, testInfo) => {
    await scene(page, variant);
    await shot(page, testInfo, `2-prepared-bar-desktop-${variant}`);
  });
  test.describe(`phone ${variant}`, () => {
    test.use({ viewport: { width: 390, height: 844 } });
    test(`prepared bar phone ${variant}`, async ({ page }, testInfo) => {
      await scene(page, variant);
      await shot(page, testInfo, `2-prepared-bar-phone-${variant}`);
    });
  });
}
