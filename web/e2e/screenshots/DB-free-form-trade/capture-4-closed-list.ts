import { test } from "@playwright/test";
import { click, openDesk, shotDesk } from "../AG-trade-staging-desk/_trade";

// For contrast: a server without the free_form flag keeps the closed list, so a combination off the list (2 trade goods for 1 commodity) is refused.
test("closed list: the same staging cannot be proposed", async ({ page }, testInfo) => {
  await openDesk(page);
  for (let i = 0; i < 2; i++) await click(page, "stage-give-tg-inc");
  await click(page, "stage-receive-cm-inc");
  await page.getByTestId("stage-status-invalid").waitFor();
  await shotDesk(page, testInfo, "4-closed-list");
});
