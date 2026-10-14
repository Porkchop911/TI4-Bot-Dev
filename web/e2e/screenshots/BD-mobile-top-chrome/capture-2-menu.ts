import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { measureTop, openMenu, openScene, prefix, SIZES, type Situation, type SizeName } from "./scene";

// The menu sheet: what it lists, that every row is a 44 px target, and that the bar keeps a cue.
// There is no menu on the base commit, so there are no before shots here.
test.skip(prefix === "before", "the menu does not exist on the base commit");
test.describe.configure({ retries: 2 });

const cases: Array<[SizeName, Situation]> = [
  ["390x844", "your-turn"],
  ["390x844", "waiting"],
  ["360x740", "waiting"],
  ["844x390", "your-turn"],
];

for (const [size, situation] of cases) {
  test(`menu open ${situation} ${size}`, async ({ page }, testInfo) => {
    await openScene(page, size, situation);
    const bar = await measureTop(page);
    expect(bar.header!.h).toBeLessThanOrEqual(40);
    expect(bar.chrome!.h).toBe(0);
    expect(bar.docScrollW).toBeLessThanOrEqual(SIZES[size].width);
    // The cue on the menu button.
    if (situation === "your-turn") await expect(page.getByTestId("top-menu-dot")).toHaveAttribute("data-kind", "decision");
    else await expect(page.getByTestId("top-menu-dot")).toHaveCount(0);

    await openMenu(page);
    const sheet = page.getByTestId("top-menu-sheet");
    for (const id of ["technology-modal-button", "objectives-modal-button", "overlay-btn-none", "overlay-btn-economy", "top-menu-close"]) {
      const box = await page.getByTestId(id).boundingBox();
      expect(box, id).not.toBeNull();
      expect(box!.height, `${id} height`).toBeGreaterThanOrEqual(44);
    }
    for (const title of ["Zoom In", "Zoom Out", "Reset Pan & Zoom"]) {
      const box = await sheet.locator(`button[title="${title}"]`).boundingBox();
      expect(box!.height, title).toBeGreaterThanOrEqual(44);
      expect(box!.width, title).toBeGreaterThanOrEqual(44);
    }
    await expect(sheet.getByText("Who’s who")).toBeVisible();
    const sheetBox = (await sheet.boundingBox())!;
    expect(sheetBox.y + sheetBox.height).toBeLessThanOrEqual(SIZES[size].height + 1);
    console.log(`MEASURE ${prefix} menu ${situation} ${size} ${JSON.stringify({ sheetTop: Math.round(sheetBox.y), sheetH: Math.round(sheetBox.height) })}`);
    await shot(page, testInfo, `${prefix}-menu-${situation}-${size}`);
  });
}

test("menu: Technologies opens the overlay and closes the sheet", async ({ page }) => {
  await openScene(page, "390x844", "waiting");
  await openMenu(page);
  await page.getByTestId("technology-modal-button").click();
  await expect(page.getByTestId("technology-modal")).toBeVisible();
  await expect(page.getByTestId("top-menu-sheet")).toHaveCount(0);
});

test("menu: zoom keeps its state while the sheet is closed and reopened", async ({ page }, testInfo) => {
  await openScene(page, "390x844", "waiting");
  const scaleOf = () =>
    page.getByTestId("ti4-board-svg").locator("> g").first().getAttribute("transform");
  const before = await scaleOf();
  await openMenu(page);
  await page.locator('[data-testid="top-menu-sheet"] button[title="Zoom In"]').click();
  await page.locator('[data-testid="top-menu-sheet"] button[title="Zoom In"]').click();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("top-menu-sheet")).toHaveCount(0);
  expect(await scaleOf()).not.toBe(before);
  await shot(page, testInfo, `${prefix}-zoomed-390x844`);
  await openMenu(page);
  await page.locator('[data-testid="top-menu-sheet"] button[title="Reset Pan & Zoom"]').click();
  await page.keyboard.press("Escape");
  expect(await scaleOf()).toBe(before);
});
