import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { COMPONENTS, PHONES, checking, expectReachable, openTurnMenu, placement, prefix } from "./scene";

// Several components on a phone: every row on screen and tappable, each says what it does.
for (const phone of PHONES) {
  test(`components at ${phone.name}`, async ({ page }, testInfo) => {
    await openTurnMenu(page, phone.width, phone.height);
    await page.getByTestId("turn-bar-components").click();
    const menu = page.getByTestId("turn-bar-menu-components");
    await menu.waitFor();
    console.log(`components heading: ${JSON.stringify(await placement(menu.locator("h4").first()))}`);
    await shot(page, testInfo, `${prefix}-components-${phone.name}`);
    for (const option of COMPONENTS) {
      const row = page.getByTestId(`turn-bar-item-${option.id}`);
      await expectReachable(row, option.id);
      if (checking) {
        const detail = row.getByTestId("turn-bar-row-text");
        await expect(detail, `${option.id} says what it does`).toBeVisible();
        expect((await detail.innerText()).length, `${option.id} rules text`).toBeGreaterThan(20);
      }
    }
    if (checking) await shot(page, testInfo, `${prefix}-components-${phone.name}-scrolled`);
  });
}

test("components on desktop", async ({ page }, testInfo) => {
  await openTurnMenu(page, 1280, 900);
  await page.getByTestId("turn-bar-components").click();
  await page.getByTestId("turn-bar-menu-components").waitFor();
  for (const option of COMPONENTS) await expectReachable(page.getByTestId(`turn-bar-item-${option.id}`), option.id);
  await page.getByTestId(`turn-bar-item-${COMPONENTS[0].id}`).scrollIntoViewIfNeeded();
  await shot(page, testInfo, `${prefix}-components-desktop`);
});
