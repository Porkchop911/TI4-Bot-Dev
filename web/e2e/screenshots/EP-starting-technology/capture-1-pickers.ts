import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { ARGENT_FIRST, ARGENT_SECOND, KELERES, SIZES, WINNU, openScene, type SizeName } from "./scene";

// The starting-technology question of a new game, as the Winnu, the Argent Flight and the Council
// Keleres see it (a mocked decision with the options, details and context the engine sends).
for (const size of Object.keys(SIZES) as SizeName[]) {
  test(`winnu pick one ${size}`, async ({ page }, testInfo) => {
    await openScene(page, size, WINNU);
    await expect(page.getByTestId("starting-technology-title")).toHaveText("Starting technology");
    await expect(page.getByTestId("technology-pick-panel")).toContainText("Choose one technology");
    await expect(page.getByTestId("choice-option")).toHaveCount(8);
    await expect(page.getByTestId("technology-pick-note").first()).toBeVisible();
    await shot(page, testInfo, `1-winnu-${size}`);
  });

  test(`argent first pick ${size}`, async ({ page }, testInfo) => {
    await openScene(page, size, ARGENT_FIRST);
    await expect(page.getByTestId("starting-technology-title")).toHaveText("Starting technology · 1 of 2");
    await expect(page.getByTestId("technology-pick-panel")).toContainText("stays in the technology deck");
    await expect(page.getByTestId("choice-option")).toHaveCount(3);
    await shot(page, testInfo, `2-argent-first-${size}`);
  });

  test(`argent second pick ${size}`, async ({ page }, testInfo) => {
    await openScene(page, size, ARGENT_SECOND);
    await expect(page.getByTestId("starting-technology-title")).toHaveText("Starting technology · 2 of 2");
    await expect(page.getByTestId("technology-pick-panel")).toContainText("You already start with Sarween Tools");
    await expect(page.getByTestId("choice-option")).toHaveCount(2);
    await shot(page, testInfo, `3-argent-second-${size}`);
  });

  test(`keleres pick ${size}`, async ({ page }, testInfo) => {
    await openScene(page, size, KELERES);
    await expect(page.getByTestId("technology-pick-panel")).toContainText("another player starts with");
    await expect(page.getByTestId("technology-pick-holders").first()).toBeVisible();
    await shot(page, testInfo, `4-keleres-${size}`);
  });

  test(`selected option and confirm ${size}`, async ({ page }, testInfo) => {
    await openScene(page, size, WINNU);
    await page.locator('[data-option-id="st"]').click();
    await expect(page.locator('[data-option-id="st"]')).toHaveClass(/card--selected/);
    await expect(page.getByTestId("submit-choice-button")).toBeEnabled();
    await shot(page, testInfo, `5-confirm-${size}`);
  });
}
