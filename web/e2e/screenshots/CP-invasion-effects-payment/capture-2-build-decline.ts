import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { freelancersBuild, openInvasionChoice } from "./invasion";

// Building as a consequence of invasion (Freelancers, explored on a planet just taken): every unit
// by name and cost, and a decline that builds nothing.

for (const [size, viewport] of [
  ["desktop", null],
  ["390x844", { width: 390, height: 844 }],
] as const) {
  test(`build after invasion with decline, ${size}`, async ({ page }, testInfo) => {
    if (viewport) await page.setViewportSize(viewport);
    const sent: string[] = [];
    await openInvasionChoice(page, "finalizing_control", freelancersBuild, (message) =>
      sent.push(JSON.stringify(message)),
    );
    const card = page.getByTestId("invasion-effect-card");
    await expect(card).toContainText("Freelancers");
    await expect(card.getByRole("button", { name: /Produce Destroyer/ })).toContainText("cost 1");
    const decline = page.getByTestId("invasion-effect-decline");
    await expect(decline).toHaveText("Decline: build nothing");
    if (viewport) {
      const scroll = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
      expect(scroll).toBeLessThanOrEqual(0);
    }
    await decline.scrollIntoViewIfNeeded();
    await shot(page, testInfo, `6-build-decline-${size}`);
    await decline.click();
    await expect.poll(() => sent.some((m) => m.includes('"option_id":"decline"'))).toBe(true);
  });
}
