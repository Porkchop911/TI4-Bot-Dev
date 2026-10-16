import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { opponent } from "../_shared/players";
import { solSeat } from "../U-unit-faction-info/fixtures";
import { prefix, record } from "./shared";

// Bug 3: the Faction card hung below a 1280x720 window (bottom edge at 787). Portal popovers now
// slide up so the whole card stays on screen; it scrolls inside its own max-height.
for (const vp of [
  { id: "1280x720", width: 1280, height: 720 },
  { id: "phone-390x844", width: 390, height: 844 },
]) {
  test(`faction card fits ${vp.id}`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: vp.width, height: vp.height });
    await openMockedGame(page, { players: [solSeat(), opponent] });
    const phone = vp.width < 600;
    // On a phone the player sheet lives in a drawer.
    if (phone) await page.getByTestId("player-sheet-toggle").click();
    const button = page.getByTestId("faction-info-button").first();
    await button.scrollIntoViewIfNeeded();
    await button.click();
    const card = page.getByTestId("faction-info-card").locator("xpath=ancestor::*[@role='dialog'][1]");
    await card.waitFor();
    await page.waitForTimeout(250);
    const box = await card.boundingBox();
    record(`faction-card-${vp.id}`, {
      top: Math.round(box!.y),
      bottom: Math.round(box!.y + box!.height),
      viewportHeight: vp.height,
      fits: box!.y >= 0 && box!.y + box!.height <= vp.height,
    });
    await shot(page, testInfo, `${prefix}-3-faction-card-${vp.id}`);
  });
}
