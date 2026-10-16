import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { PHONES, assertFloatingButtonsClear, barBox, openTurnMenu, prefix } from "./scene";

// Expanded is the default (fresh browser context, nothing stored) and must look as it did before.
for (const phone of PHONES) {
  test(`action pane expanded at ${phone.name}`, async ({ page }, testInfo) => {
    await openTurnMenu(page, phone.width, phone.height);
    expect((await barBox(page)).height).toBeGreaterThan(100);
    await assertFloatingButtonsClear(page);
    await shot(page, testInfo, `${prefix}-expanded-${phone.name}`);
  });
}
