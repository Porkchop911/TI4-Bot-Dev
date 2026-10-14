import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openExtra } from "./extras";
import { prefix, SIZES, type SizeName } from "./scene";

// Tapping a system (or a card in the player sheet) opens a panel that covers most of the map and
// could only be closed. On a phone it now folds to a slim bar next to the Players / Events buttons;
// the map stays tappable and the bar follows the tapped system. TI4_SHOT_PREFIX=before is how the
// "before" shots were taken, on the base commit.
const sizes: SizeName[] = ["390x844", "360x740", "844x390"];

for (const size of sizes)
  test(`${prefix} system panel ${size}`, async ({ page }, testInfo) => {
    await openExtra(page, "system-inspector", size);
    const panel = page.getByTestId("system-inspector");
    const open = (await panel.boundingBox())!;
    const vp = SIZES[size];
    console.log(`MEASURE ${size}: open panel ${Math.round(open.width)}x${Math.round(open.height)} = ${Math.round(((open.width * open.height) / (vp.width * vp.height)) * 100)}% of the screen`);
    await shot(page, testInfo, `${prefix}-system-open-${size}`);
    if (prefix === "before") return;
    await page.getByTestId("system-inspector-fold").click();
    await expect(panel).toHaveClass(/detail-panel--folded/);
    const folded = (await panel.boundingBox())!;
    console.log(`MEASURE ${size}: folded panel ${Math.round(folded.width)}x${Math.round(folded.height)}`);
    expect(folded.height).toBeLessThan(80);
    expect((await page.getByTestId("system-inspector-fold").boundingBox())!.height).toBeGreaterThanOrEqual(43.5);
    expect((await page.getByTestId("close-inspector-button").boundingBox())!.height).toBeGreaterThanOrEqual(43.5);
    // The map is tappable through where the panel was: another system replaces the title and stays folded.
    await page.getByTestId("system-hex-26").click();
    await expect(page.getByTestId("inspector-system-title")).toBeAttached();
    await expect(panel).toHaveClass(/detail-panel--folded/);
    await expect(panel.getByRole("heading", { level: 2 })).toContainText("#26");
    await shot(page, testInfo, `${prefix}-system-folded-${size}`);
    await page.getByTestId("system-inspector-fold").click();
    await expect(panel).not.toHaveClass(/detail-panel--folded/);
  });

test(`${prefix} card panel 390x844`, async ({ page }, testInfo) => {
  await openExtra(page, "card-details", "390x844");
  await shot(page, testInfo, `${prefix}-card-open-390x844`);
  if (prefix === "before") return;
  await page.getByTestId("detail-panel-fold").click();
  await shot(page, testInfo, `${prefix}-card-folded-390x844`);
});
