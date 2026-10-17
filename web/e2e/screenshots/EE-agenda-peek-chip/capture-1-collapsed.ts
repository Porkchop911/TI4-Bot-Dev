import { expect, test } from "@playwright/test";
import { VIEWPORTS, assertTurnBarReachable, openTurnWithPeek, prefix, takeShot } from "./scene";

// The default state: what a player sees right after Insider Information, with their turn menu open.
for (const viewport of VIEWPORTS)
  test(`peek with the turn bar at ${viewport.id}`, async ({ page }, testInfo) => {
    await openTurnWithPeek(page, viewport);
    const peek = await page.getByTestId("agenda-peek").boundingBox();
    const bar = await page.getByTestId("turn-action-bar").boundingBox();
    console.log(`MEASURE ${viewport.id}: peek ${Math.round(peek!.height)}px tall, ${Math.round(peek!.width)} wide; bar y=${Math.round(bar!.y)} h=${Math.round(bar!.height)}`);
    if (prefix === "after") {
      expect(peek!.height).toBeLessThanOrEqual(64);
      await assertTurnBarReachable(page);
    }
    await takeShot(page, testInfo, "collapsed", viewport);
  });
