import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { galleryPlayers } from "../../../src/dev/galleryBoard";
import { plagueBoard, plagueChoice, prefix } from "./scene";

// fb_1792038952: Plague's planet chips showed resources/influence; Plague cares about infantry.
for (const size of [
  { name: "desktop", width: 1440, height: 900 },
  { name: "390x844", width: 390, height: 844 },
]) {
  test(`plague planet chips ${size.name}`, async ({ page }, testInfo) => {
    await page.setViewportSize(size);
    await openMockedGame(page, { board: plagueBoard, players: galleryPlayers, choice: plagueChoice });
    const bar = page.getByTestId("planet-selection-bar");
    await bar.waitFor();
    if (prefix === "after") {
      await expect(page.getByTestId("planet-chip-jord")).toContainText("4 infantry");
      await expect(page.getByTestId("planet-chip-exhausted")).toContainText("1 infantry");
    }
    await shot(page, testInfo, `${prefix}-plague-chips-${size.name}`, size.name === "desktop" ? { of: bar, pad: 8 } : {});
  });
}
