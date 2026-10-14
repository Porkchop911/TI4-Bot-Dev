import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openCase, prefix, SIZES, type SizeName } from "./scene";

// With the payment list minimized, the confirm bar stays over the map while the planets that pay are
// tapped on it. On a phone it carried a hint line and a per-source detail line and wrapped its buttons
// into three rows (about 220 px of 740); it now drops the two lines and keeps 44 px buttons.
// TI4_SHOT_PREFIX=before is how the "before" shots were taken, on the base commit.
const sizes: SizeName[] = ["390x844", "360x740", "844x390"];

for (const size of sizes)
  test(`${prefix} payment bar ${size}`, async ({ page }, testInfo) => {
    await openCase(page, "payment", size);
    await page.getByTestId("close-payment-drawer").click();
    const bar = page.getByTestId("payment-bar");
    await bar.waitFor();
    const box = (await bar.boundingBox())!;
    const vp = SIZES[size];
    console.log(`MEASURE payment bar ${size}: ${Math.round(box.width)}x${Math.round(box.height)} = ${Math.round((box.height / vp.height) * 100)}% of the screen height`);
    await shot(page, testInfo, `${prefix}-payment-bar-${size}`);
    if (prefix === "before") return;
    for (const id of ["confirm-payment-btn", "auto-pay-btn", "resume-decision-btn"])
      expect((await page.getByTestId(id).boundingBox())!.height, id).toBeGreaterThanOrEqual(43.5);
    // Planets on the map stay tappable above the bar.
    const hit = await page.evaluate(() => {
      const hexes = Array.from(document.querySelectorAll<SVGElement>('[data-testid^="system-hex-"]'));
      return hexes.filter((hex) => {
        const r = hex.getBoundingClientRect();
        const top = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
        return !!top?.closest('[data-testid="ti4-board-svg"]');
      }).length;
    });
    console.log(`MEASURE payment bar ${size}: ${hit} systems reachable above the bar`);
    // A landscape phone has almost no map left once the header and the toolbars are counted (the top
    // chrome is minimized separately); the portrait layouts keep most of it.
    if (size !== "844x390") expect(hit, "systems reachable above the bar").toBeGreaterThan(5);
  });
