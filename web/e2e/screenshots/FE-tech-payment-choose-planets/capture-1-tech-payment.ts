import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import {
  assertConfirmReachable,
  assertNoHorizontalOverflow,
  isPhone,
  newPage,
  openPaymentOf,
  SIZES,
} from "../EQ-payment-single-column/scene";

// After picking a technology with the Technology secondary (rules version 6) the engine asks the
// ordinary pay_resources question for the 4-resource price: the exhaust-planets dialog.
for (const s of SIZES) {
  if (s.label !== "390x844" && s.label !== "desktop") continue;
  for (const stage of [0, 2]) {
    test(`tech payment ${stage ? "staged" : "open"} ${s.label}`, async ({ browser, baseURL }, testInfo) => {
      const { context, page } = await newPage(browser, s, baseURL);
      await openPaymentOf(page, "resources", 4, 4);
      const cards = page.locator('label[data-testid^="planet-card-"]');
      for (let i = 0; i < stage; i++) await cards.nth(i).click();
      if (isPhone(s)) {
        await assertNoHorizontalOverflow(page);
        await assertConfirmReachable(page);
      }
      await shot(page, testInfo, `tech-payment-${stage ? "staged" : "open"}-${s.label}`);
      await context.close();
    });
  }
}
