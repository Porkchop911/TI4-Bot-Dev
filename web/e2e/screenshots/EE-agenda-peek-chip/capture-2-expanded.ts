import { expect, test } from "@playwright/test";
import { VIEWPORTS, openTurnWithPeek, prefix, takeShot } from "./scene";

// Opened on purpose: the chip expands to the three cards; Escape closes the card.
test.skip(prefix === "before", "the card was always open on the base commit");
for (const viewport of VIEWPORTS)
  test(`peek expanded at ${viewport.id}`, async ({ page }, testInfo) => {
    await openTurnWithPeek(page, viewport);
    await page.getByTestId("agenda-peek-toggle").click();
    await expect(page.getByTestId("agenda-peek-entry")).toHaveCount(3);
    const peek = await page.getByTestId("agenda-peek").boundingBox();
    expect(peek!.height).toBeLessThanOrEqual(viewport.height * 0.5 + 2);
    await takeShot(page, testInfo, "expanded", viewport);
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("agenda-peek")).toHaveCount(0);
  });
