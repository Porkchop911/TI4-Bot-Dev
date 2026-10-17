import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { SIZES, layoutCards, openForm, openPicker } from "./mapSelect";
import { requireEngine } from "../DO-offline-game/offlineGame";

// The offline setup form with the map row, and the picker behind "Choose map": the lobby's strip
// of cards and large picture, answered by the real engine (wasm) instead of the server.
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`setup form and map picker ${size}`, async ({ page }, testInfo) => {
    test.setTimeout(180_000);
    requireEngine();
    await page.setViewportSize(viewport);
    await openForm(page, 4);
    await expect(page.getByTestId("offline-map-name")).toHaveText("Recommended layout");
    await shot(page, testInfo, `1-form-default-${size}`);

    await openPicker(page);
    const cards = await layoutCards(page);
    expect(cards.filter((c) => c.on)).toHaveLength(1); // the recommended layout is shown as selected
    await shot(page, testInfo, `2-picker-recommended-${size}`);

    const other = cards.find((c) => !c.on && /Hyperlane/i.test(c.id)) ?? cards.find((c) => !c.on)!;
    await page.getByTestId(`map-card-${other.id}`).click();
    await expect(page.getByTestId(`map-card-${other.id}`)).toHaveAttribute("aria-pressed", "true");
    await expect(page.getByTestId("map-preview-board")).toBeVisible();
    await shot(page, testInfo, `3-picker-picked-${size}`);

    await page.getByTestId("map-card-random").click();
    await expect(page.getByTestId("map-card-random")).toHaveAttribute("aria-pressed", "true");
    await shot(page, testInfo, `4-picker-random-${size}`);

    await page.getByTestId(`map-card-${other.id}`).click();
    await page.getByTestId("map-picker-done").click();
    await expect(page.getByTestId("offline-map-name")).not.toHaveText("Recommended layout");
    await shot(page, testInfo, `5-form-chosen-${size}`);
    if (size === "390x844") {
      expect(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)).toBe(false);
    }
  });
}

test("six players get six-player layouts, and a pick is forgotten when the size changes", async ({ page }, testInfo) => {
  test.setTimeout(180_000);
  requireEngine();
  await page.setViewportSize(SIZES.desktop);
  await openForm(page, 6);
  await openPicker(page);
  await expect(page.locator(".map-picker__seats li")).toHaveCount(6);
  await shot(page, testInfo, "6-picker-six-players-desktop");
  const other = (await layoutCards(page)).find((c) => !c.on)!;
  await page.getByTestId(`map-card-${other.id}`).click();
  await page.getByTestId("map-picker-done").click();
  await expect(page.getByTestId("offline-map-name")).not.toHaveText("Recommended layout");
  await page.getByTestId("offline-player-count").selectOption("4");
  await expect(page.getByTestId("offline-map-name")).toHaveText("Recommended layout");
});
