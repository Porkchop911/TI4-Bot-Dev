import { expect, test } from "@playwright/test";
import { openLogFully } from "../_shared/eventLog";
import { shot } from "../_shared/shot";
import { openSeated, openSeatedLobby, prefix, type SizeName } from "./scene";

const SIZES: SizeName[] = ["desktop", "phone"];

for (const n of [3, 6, 8]) {
  for (const size of SIZES) {
    test(`map ${n} players ${size}`, async ({ page }, testInfo) => {
      await openSeated(page, n, size);
      await expect(page.getByTestId("board")).toBeVisible().catch(() => undefined);
      await shot(page, testInfo, `${prefix}-map-${n}p-${size}`);
    });

    test(`player sheets ${n} players ${size}`, async ({ page }, testInfo) => {
      await openSeated(page, n, size);
      // A tall desktop window so every seat's sheet is in view; the phone drawer scrolls instead.
      if (size === "desktop") await page.setViewportSize({ width: 1440, height: 400 + n * 420 });
      if (size === "phone") await page.getByTestId("player-sheet-toggle").click();
      await expect(page.getByTestId("player-card")).toHaveCount(n);
      await shot(page, testInfo, `${prefix}-sheets-${n}p-${size}`);
    });

    test(`event log ${n} players ${size}`, async ({ page }, testInfo) => {
      await openSeated(page, n, size);
      if (size === "phone") {
        await page.getByTestId("event-log-mobile-toggle").click();
        const list = page.getByTestId("event-log-list");
        await list.waitFor();
        await page.waitForTimeout(800);
        for (let i = 0; i < 6; i++) {
          const closed = list.locator('button[aria-expanded="false"]');
          if (!(await closed.count())) break;
          // The phone's button bar overlaps the lower rows, so click through the DOM.
          await closed.first().dispatchEvent("click");
        }
      } else await openLogFully(page);
      await shot(page, testInfo, `${prefix}-log-${n}p-${size}`);
    });

    test(`lobby ${n} players ${size}`, async ({ page }, testInfo) => {
      await openSeatedLobby(page, n, size);
      await expect(page.getByText(/Position 1/).first()).toBeVisible();
      await shot(page, testInfo, `${prefix}-lobby-${n}p-${size}`);
    });
  }
}
