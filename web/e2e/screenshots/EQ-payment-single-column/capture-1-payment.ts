import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { assertConfirmReachable, assertFooterGrid, assertNoHorizontalOverflow, assertSingleColumn, isPhone, newPage, openPaymentOf, prefix, SIZES } from "./scene";

const VARIANTS = [
  { name: "influence-short", kind: "influence", owed: 8, planets: 4, stage: 1 },
  { name: "resources", kind: "resources", owed: 5, planets: 3, stage: 0 },
  { name: "paid", kind: "influence", owed: 3, planets: 4, stage: 3 },
  { name: "many-planets", kind: "influence", owed: 12, planets: 10, stage: 0 },
] as const;


for (const v of VARIANTS) {
  for (const s of SIZES) {
    if (v.name !== "influence-short" && s.label !== "390x844" && s.label !== "desktop") continue;
    test(`${prefix} ${v.name} ${s.label}`, async ({ browser, baseURL }, testInfo) => {
      const { context, page } = await newPage(browser, s, baseURL);
      await openPaymentOf(page, v.kind, v.owed, v.planets);
      const cards = page.locator('label[data-testid^="planet-card-"]');
      for (let i = 0; i < v.stage; i++) await cards.nth(i).click();
      if (prefix === "after" && isPhone(s)) {
        await assertNoHorizontalOverflow(page);
        await assertSingleColumn(page);
        await assertConfirmReachable(page);
        if (s.label === "390x844" || s.label === "360x740") await assertFooterGrid(page);
      }
      await shot(page, testInfo, `${prefix}-${v.name}-${s.label}`);
      await context.close();
    });
  }
}
