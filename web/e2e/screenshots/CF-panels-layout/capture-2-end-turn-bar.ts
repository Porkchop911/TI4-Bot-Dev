import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openTurn, prefix } from "./scene";

test("desktop: Pass and End turn sit right after the other buttons", async ({ page }, testInfo) => {
  await openTurn(page, { width: 1440, height: 900 });
  const bar = page.getByTestId("turn-action-bar");
  if (prefix === "after") {
    const buttons = bar.locator(".turn-bar__actions .turn-bar__btn");
    const boxes = await Promise.all((await buttons.all()).map((b) => b.boundingBox()));
    for (let i = 1; i < boxes.length; i++) {
      const gap = boxes[i]!.x - (boxes[i - 1]!.x + boxes[i - 1]!.width);
      expect(gap, `gap before button ${i}`).toBeLessThan(40);
    }
  }
  await shot(page, testInfo, `${prefix}-desktop-bar`, { of: bar, pad: 4 });
});
