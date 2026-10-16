import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { PHONES, checking, expectReachable, openTurnMenu, placement, prefix } from "./scene";

const FACTIONS = ["hacan", "jolnar", "letnev", "naalu", "xxcha"];

// Trade on a phone: the partner picker must show every partner (name, holdings, Open) on screen.
for (const phone of PHONES) {
  test(`trade partners at ${phone.name}`, async ({ page }, testInfo) => {
    await openTurnMenu(page, phone.width, phone.height);
    await page.getByTestId("turn-bar-trade").click();
    const menu = page.getByTestId("turn-bar-menu-trade");
    await menu.waitFor();
    const heading = menu.locator("h4").first();
    console.log(`trade heading: ${JSON.stringify(await placement(heading))}`);
    await shot(page, testInfo, `${prefix}-trade-${phone.name}`);
    if (checking) expect((await placement(heading)).inViewport, "menu heading on screen").toBe(true);
    for (const faction of FACTIONS) {
      const card = page.getByTestId(`turn-bar-partner-${faction}`);
      await card.scrollIntoViewIfNeeded();
      const name = card.locator(".turn-bar__partner-name > span").first();
      const p = await placement(name);
      console.log(`${faction} name: ${JSON.stringify(p)}`);
      if (checking) {
        expect(p.inViewport, `${faction} name on screen`).toBe(true);
        expect(p.covered, `${faction} name covered`).toBeNull();
      }
      await expectReachable(page.getByTestId(`turn-bar-open-${faction}`), `${faction} Open`);
    }
    if (checking) {
      // The last partner, scrolled into view inside the menu.
      await shot(page, testInfo, `${prefix}-trade-${phone.name}-scrolled`);
    }
  });
}
