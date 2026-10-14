import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";
import { shot } from "../_shared/shot";

// The technology viewer filters by colour and by what the viewer can research now.
const SIZES = [["desktop", { width: 1440, height: 900 }], ["390x844", { width: 390, height: 844 }]] as const;

async function open(page: import("@playwright/test").Page, viewport: { width: number; height: number }) {
  await page.setViewportSize(viewport);
  await openMockedGame(page, { players: [playerWithHand({ technologies: ["amd", "st"] }), opponent] });
  const menu = page.getByTestId("top-menu-button");
  if (await menu.count()) await menu.click();
  await page.getByTestId("technology-modal-button").click();
  await page.getByTestId("technology-modal").waitFor();
  await page.waitForTimeout(500);
}

for (const [size, viewport] of SIZES) {
  test(`filter chips ${size}`, async ({ page }, testInfo) => {
    await open(page, viewport);
    await shot(page, testInfo, `after-unfiltered-${size}`);
  });
  test(`colour filter ${size}`, async ({ page }, testInfo) => {
    await open(page, viewport);
    await page.getByTestId("tech-filter-biotic").click();
    await expect(page.getByTestId("tech-card-amd")).toHaveCount(0);
    await expect(page.getByTestId("tech-card-nm")).toBeVisible();
    await shot(page, testInfo, `after-green-${size}`);
  });
  test(`researchable now ${size}`, async ({ page }, testInfo) => {
    await open(page, viewport);
    await page.getByTestId("tech-filter-available").click();
    await expect(page.getByTestId("tech-card-amd")).toHaveCount(0);
    await expect(page.getByTestId("tech-card-ws")).toHaveCount(0);
    await shot(page, testInfo, `after-researchable-${size}`);
  });
  test(`colour and researchable ${size}`, async ({ page }, testInfo) => {
    await open(page, viewport);
    await page.getByTestId("tech-filter-propulsion").click();
    await page.getByTestId("tech-filter-available").click();
    await shot(page, testInfo, `after-blue-researchable-${size}`);
  });
}
