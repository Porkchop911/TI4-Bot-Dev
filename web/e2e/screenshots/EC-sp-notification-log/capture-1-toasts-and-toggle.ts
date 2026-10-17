import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { VIEWPORTS, checking, coveredTurnBarControls, logToggle, prefix, reachToasts, startSeeded, toastCount } from "./scene";

// The toast stack, the notification log toggle and swipe-to-dismiss in a real offline game.
for (const viewport of VIEWPORTS) {
  test(`toasts and the notification log at ${viewport.name}`, async ({ page }, testInfo) => {
    test.setTimeout(120_000);
    const gameId = await startSeeded(page, viewport);
    await reachToasts(page, gameId, 2);
    console.log(`toasts on screen: ${await toastCount(page)}; covered: ${JSON.stringify(await coveredTurnBarControls(page))}`);
    await shot(page, testInfo, `${prefix}-toasts-${viewport.name}`);

    if (checking) {
      // Toasts leave the turn bar's controls alone, and the log toggle sits in the row of the objective buttons.
      expect(await coveredTurnBarControls(page), "turn bar controls covered").toEqual([]);
      const toggle = logToggle(page, viewport);
      await expect(toggle).toBeVisible();
      await expect(toggle).toHaveText(/^\d*\+?$/); // no words: an icon (and a tiny unread count)
      await expect(toggle).toHaveAttribute("aria-label", /Notifications/);
      const objectives = page.getByTestId(viewport.name === "desktop" ? "objectives-modal-button" : "objectives-mobile-button");
      const a = (await toggle.boundingBox())!;
      const b = (await objectives.boundingBox())!;
      expect(Math.abs(a.y + a.height / 2 - (b.y + b.height / 2)), "same row as Objectives").toBeLessThan(6);
      await expect(page.getByTestId("toast-history-pill")).toHaveCount(0);
    }

    const row = page.locator(viewport.name === "desktop" ? ".game-header__actions" : ".app-shell__mobile-actions");
    if (checking) await shot(page, testInfo, `${prefix}-objectives-row-${viewport.name}`, { of: row, pad: 8 });

    // The log, newest first.
    await logToggle(page, viewport).click();
    const list = page.getByTestId("toast-history-list");
    await expect(list).toBeVisible();
    const items = page.getByTestId("toast-history-item");
    console.log(`log entries: ${await items.count()}`);
    await shot(page, testInfo, `${prefix}-log-open-${viewport.name}`);
    await logToggle(page, viewport).click();
  });

  if (!checking) continue;
  test(`swipe a toast away at ${viewport.name}`, async ({ page }, testInfo) => {
    test.setTimeout(120_000);
    const gameId = await startSeeded(page, viewport);
    await reachToasts(page, gameId, 2);
    const toasts = page.getByTestId("corner-toast");
    const before = await toasts.count();
    const target = toasts.first();
    const text = (await target.locator(".toast-value").textContent())!;
    const box = (await target.boundingBox())!;
    const y = box.y + box.height / 2;
    // Mouse drag, half way: the toast follows the pointer and fades.
    await page.mouse.move(box.x + box.width / 2, y);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 - 40, y, { steps: 4 });
    await page.mouse.move(box.x + box.width / 2 - 60, y, { steps: 4 });
    await shot(page, testInfo, `${prefix}-swiping-${viewport.name}`);
    await page.mouse.move(box.x + box.width / 2 - 110, y, { steps: 3 });
    await page.mouse.up();
    await expect.poll(() => toasts.count(), { timeout: 3000 }).toBeLessThan(before);
    await shot(page, testInfo, `${prefix}-swiped-${viewport.name}`);
    // It is gone from the screen, not from the log.
    await logToggle(page, viewport).click();
    await expect(page.getByTestId("toast-history-item").filter({ hasText: text.trim().slice(0, 24) }).first()).toBeVisible();
  });
}
