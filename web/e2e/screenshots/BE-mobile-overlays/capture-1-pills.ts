import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openCase, prefix, SIZES, type SizeName } from "./scene";

// A minimized decision or combat collapses to a pill. On a phone the pill used to stretch from the
// middle of the screen to the right edge, cut its label off and lie over the Players / Events buttons
// (and, in landscape, over the docked prepare banner's corner). TI4_SHOT_PREFIX=before is how the
// "before" shots were taken, on the base commit.
const scenes = [
  { id: "decision", title: "tactical movement", minimize: "close-movement-tray", pill: "choice-minimized-pill", resume: "resume-decision-btn" },
  { id: "combat", title: "combat casualty", minimize: "close-combat-modal", pill: "combat-docked-pill", resume: "resume-combat-btn" },
];
const sizes: SizeName[] = ["390x844", "360x740", "844x390"];

for (const scene of scenes)
  for (const size of sizes)
    test(`${prefix} minimized ${scene.id} pill ${size}`, async ({ page }, testInfo) => {
      await openCase(page, scene.title, size);
      await page.getByTestId(scene.minimize).click();
      const pill = page.getByTestId(scene.pill);
      await pill.waitFor();
      const box = (await pill.boundingBox())!;
      const resume = (await page.getByTestId(scene.resume).boundingBox())!;
      const vp = SIZES[size];
      const buttons = await page.locator(".app-shell__mobile-actions").boundingBox().catch(() => null);
      const overlap = buttons
        ? box.x < buttons.x + buttons.width && buttons.x < box.x + box.width && box.y < buttons.y + buttons.height && buttons.y < box.y + box.height
        : false;
      console.log(`MEASURE ${scene.id} ${size}: pill ${Math.round(box.x)},${Math.round(box.y)} ${Math.round(box.width)}x${Math.round(box.height)}; resume ${Math.round(resume.width)}x${Math.round(resume.height)}; over Players/Events: ${overlap}`);
      if (prefix !== "before") {
        expect(overlap, "the pill must not lie over the Players / Events buttons").toBe(false);
        expect(resume.height).toBeGreaterThanOrEqual(43.5);
        expect(box.x).toBeGreaterThanOrEqual(0);
        expect(box.x + box.width).toBeLessThanOrEqual(vp.width);
        // The map above stays reachable: a tap on a system lands on the map, not on the pill.
        const hit = await page.evaluate(() => {
          const el = document.querySelector('[data-testid="system-hex-18"]')!.getBoundingClientRect();
          const top = document.elementFromPoint(el.x + el.width / 2, el.y + el.height / 2);
          return !!top?.closest('[data-testid="ti4-board-svg"]');
        });
        expect(hit, "system 18 is reachable with the pill shown").toBe(true);
      }
      await shot(page, testInfo, `${prefix}-pill-${scene.id}-${size}`);
    });
