import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { noHorizontalPageScroll, openObjectives, openScene, SIZES, type SizeName } from "./scene";

// Learn the Secrets of the Cosmos: the holder sees n / 3 and which systems count.
const NAMES = ["#26 [Lodor]", "#25 [Quann]", "#28 [Tequ'ran / Torkan]"];

for (const count of [0, 1, 2, 3]) {
  for (const size of Object.keys(SIZES) as SizeName[]) {
    test(`learn the secrets ${count} ${size}`, async ({ page }, testInfo) => {
      await openScene(page, size, "ahead-1", count);
      await openObjectives(page, size);
      const progress = page.getByTestId("objectives-modal").getByTestId("secret-progress-lsc");
      await expect(progress).toContainText(
        `${count} / 3 systems adjacent to an anomaly with your ships`,
      );
      await expect(progress).toHaveAttribute("data-satisfied", count >= 3 ? "true" : "false");
      const systems = progress.getByTestId("secret-progress-systems-lsc");
      if (count === 0) {
        await expect(systems).toHaveCount(0);
      } else {
        await expect(systems).toHaveText(`Counting: ${NAMES.slice(0, count).join(", ")}`);
      }
      if (size === "390x844") expect(await noHorizontalPageScroll(page)).toBeLessThanOrEqual(0);
      await page.getByTestId("secret-row-held-lsc").scrollIntoViewIfNeeded();
      await shot(page, testInfo, `2-lsc-${count}-${size}`);
    });
  }
}

// The player sheet's private hand shows the same line.
test("learn the secrets player sheet desktop", async ({ page }, testInfo) => {
  await openScene(page, "desktop", "ahead-1", 2);
  const item = page.getByTestId("secret-objective-item-lsc");
  await expect(item).toContainText("2 / 3 systems adjacent to an anomaly with your ships");
  await expect(item).toContainText("Counting: #26 [Lodor], #25 [Quann]");
  await item.scrollIntoViewIfNeeded();
  await shot(page, testInfo, "3-lsc-player-sheet-desktop", { of: page.getByTestId("player-sheet-drawer") });
});
