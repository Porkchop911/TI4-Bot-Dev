import { expect, test } from "@playwright/test";
import { galleryBoard } from "../_shared/fixtures";
import { hoverShot } from "../_shared/hover";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";

// Hovering another player's units shows their numbers: in the system overview and in the
// tile tooltip. The rival (Hacan) has a carrier and a dreadnought in system 19.
const board = {
  ...galleryBoard,
  systems: {
    ...galleryBoard.systems,
    "19": {
      ...galleryBoard.systems["19"],
      units: [
        ...galleryBoard.systems["19"].units,
        { unit_type: "dreadnought", owner: opponent.id, damaged: false },
        { unit_type: "carrier", owner: opponent.id, damaged: false },
      ],
    },
  },
};

test("rival unit in the system overview", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent], board: board as never });
  await page.getByTestId("system-hex-19").click();
  const group = page.getByTestId(`inspector-player-space-${opponent.id}`);
  await group.waitFor();
  const chip = group.locator("[data-unit-stats]").first();
  await hoverShot(page, testInfo, "after-rival-unit-overview", chip, { tooltip: page.getByTestId("unit-info-card").first(), pad: 40 });
  await expect(page.getByTestId("unit-info-card").first()).toBeVisible();
});

test("rival units in the tile tooltip", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent], board: board as never });
  const hex = page.getByTestId("system-hex-19");
  await hex.dispatchEvent("mouseover");
  await hex.hover({ force: true });
  await page.getByTestId("tooltip-unit-stats").waitFor();
  await page.waitForTimeout(400);
  await shot(page, testInfo, "after-rival-unit-tile");
});
