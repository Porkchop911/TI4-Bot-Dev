import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { galleryPlayers } from "../../../src/dev/galleryBoard";
import { prefix } from "./scene";

// fb_1792039768: unlocking a commander told nobody what it does. Now every seat gets a toast with
// the printed ability when a commander or hero unlocks.
const withCommander = (status: string) =>
  galleryPlayers.map((p, i) =>
    i === 1 ? { ...p, leaders: { ...p.leaders, hacancommander: status } } : { ...p, leaders: { ...p.leaders, solcommander: status } },
  );

for (const size of [
  { name: "desktop", width: 1440, height: 900 },
  { name: "390x844", width: 390, height: 844 },
]) {
  test(`commander unlock toast ${size.name}`, async ({ page }, testInfo) => {
    await page.setViewportSize(size);
    const game = await openMockedGame(page, { players: withCommander("locked") });
    await page.waitForTimeout(500);
    const { events: _events, ...update } = game.snapshot;
    game.send({
      ...update,
      type: "state_update",
      game_version: 41,
      view: { ...update.view, players: withCommander("unlocked") },
    });
    if (prefix === "after") {
      const toast = page.getByTestId("corner-toast").filter({ hasText: "Claire Gibson" }).first();
      await toast.waitFor();
      await expect(toast).toContainText("At the start of a ground combat on a planet you control");
    } else {
      await page.waitForTimeout(1500);
    }
    await page.waitForTimeout(700);
    await shot(page, testInfo, `${prefix}-commander-unlocked-${size.name}`);
  });
}
