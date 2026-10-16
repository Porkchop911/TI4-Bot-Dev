import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { board, players } from "./_fixtures";

// The system overview lists what was explored there, the card and what the player got.
for (const [system, name] of [["24", "planet-gain"], ["26", "attachment"], ["39", "frontier-device"], ["41", "frontier-waiting"]] as const) {
  test(`system overview ${name}`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    const { list, state } = players();
    await openMockedGame(page, { board, players: list, state });
    await page.getByTestId(`system-hex-${system}`).click();
    const panel = page.getByTestId("system-inspector");
    await panel.waitFor();
    await expect(page.getByTestId("inspector-exploration")).toBeVisible();
    await shot(page, testInfo, `overview-${name}`, { of: panel, pad: 8 });
  });
}
