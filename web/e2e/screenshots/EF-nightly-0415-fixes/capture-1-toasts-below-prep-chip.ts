import { expect, test, type Page } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting, PRIMARY } from "../AH-secondary-prep-real-ui/prep";
import { publicEntry, pushEntry } from "../G-corner-notifications/corner";
import { combatRound, rollEntry } from "../DM-public-dice-rolls/dice";

// Nightly 2026-10-14 #12 (run s2-36-0118): a tall corner toast stack covered the "Prepare your
// secondary" chip. The column now stops below the chip. Asserted: the chip's centre is not covered
// (elementFromPoint) and every toast ends below the chip.
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";
const viewports = [
  { id: "1280x720", width: 1280, height: 720 },
  { id: "1440x900", width: 1440, height: 900 },
];

/** Steps of the primary's strategic action (same action id, so the chip stays), in different toast kinds. */
const STEPS = [
  "activated system #75 [Devils home system]",
  "moved l1z1x_dreadnought from #74 [Vega Major/Vega Minor] to #75 [Devils home system]",
  "explored Lazar (industrial): Functioning Base - nothing gained",
  "produced 2 fighter",
  "played Sabotage",
  "researched Neural Motivator",
  "gained a tactic command token",
  "scored Erect a Monument",
];

async function chipCovered(page: Page): Promise<boolean> {
  return page.getByTestId("secondary-prep-chip").first().evaluate((chip) => {
    const r = chip.getBoundingClientRect();
    const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    return !(hit && chip.contains(hit));
  });
}

for (const viewport of viewports) {
  test(`${prefix} toasts and the prepare chip ${viewport.id}`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: viewport.width, height: viewport.height });
    const game = await openWaiting(page, { name: "Politics", card: "pok3politics" });
    await page.getByTestId("ti4-board-svg").waitFor();
    const chip = page.getByTestId("secondary-prep-chip").first();
    await chip.waitFor();
    STEPS.forEach((what, index) =>
      pushEntry(
        game,
        publicEntry(`shot-step-${index}`, PRIMARY, what, {
          action_id: "action_21",
          action_type: "strategic",
          action_actor: PRIMARY,
          stage: "strategy",
        }),
      ),
    );
    // Three combat rolls in different systems: one dice toast each, as in the nightly screenshot.
    for (const system of ["24", "18", "27"])
      pushEntry(game, rollEntry({ ...combatRound(PRIMARY, [6, 7], 0), system }, { action_id: "action_21", action_type: "strategic", action_actor: PRIMARY }));
    await page.getByTestId("corner-toast").first().waitFor();
    await page.waitForTimeout(800);
    const shown = await page.getByTestId("corner-toast").count();
    console.log(`${viewport.id}: ${shown} toasts, chip covered: ${await chipCovered(page)}`);
    await shot(page, testInfo, `${prefix}-toasts-chip-${viewport.id}`);
    if (prefix === "after") {
      expect(shown, "several toasts are on screen").toBeGreaterThan(2);
      expect(await chipCovered(page), "the prepare chip is covered by a toast").toBe(false);
      const chipBox = (await chip.boundingBox())!;
      const column = (await page.getByTestId("corner-toasts").boundingBox())!;
      expect(column.y, "the toast column starts below the chip").toBeGreaterThanOrEqual(chipBox.y + chipBox.height);
      await chip.click({ timeout: 3000 });
      await page.getByTestId("prepare-banner").waitFor();
    }
  });
}
