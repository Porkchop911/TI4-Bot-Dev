import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { PHONES, assertFloatingButtonsClear, barBox, openTurnMenu, prefix } from "./scene";

// Collapsed: a slim one-line bar, the map keeps the rest. Needs the control, so no before shots.
test.skip(prefix === "before", "the control does not exist on the base commit");
for (const phone of PHONES) {
  test(`action pane collapsed at ${phone.name}`, async ({ page }, testInfo) => {
    await openTurnMenu(page, phone.width, phone.height);
    const expanded = await barBox(page);
    const toggle = page.getByTestId("turn-bar-minimize");
    expect((await toggle.boundingBox())!.height).toBeGreaterThanOrEqual(44);
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
    const collapsed = await barBox(page);
    expect(collapsed.height).toBeLessThanOrEqual(48);
    expect(collapsed.width).toBeGreaterThanOrEqual(phone.width - 1);
    await expect(page.getByTestId("turn-bar-mini-text")).toHaveText("Your turn — choose an action");
    await expect(page.getByTestId("turn-bar-tactical")).toBeHidden();
    await assertFloatingButtonsClear(page);
    console.log(`MEASURE ${phone.name} expanded=${expanded.height} collapsed=${collapsed.height} gained=${expanded.height - collapsed.height}`);
    await shot(page, testInfo, `${prefix}-collapsed-${phone.name}`);

    // The choice survives a reload; expanding again restores the buttons.
    await page.reload();
    await page.getByTestId("turn-action-bar").waitFor();
    expect((await barBox(page)).height).toBeLessThanOrEqual(48);
    await page.getByTestId("turn-bar-minimize").click();
    await expect(page.getByTestId("turn-bar-tactical")).toBeVisible();
    expect((await barBox(page)).height).toBeGreaterThan(100);
    await assertFloatingButtonsClear(page);
  });
}
