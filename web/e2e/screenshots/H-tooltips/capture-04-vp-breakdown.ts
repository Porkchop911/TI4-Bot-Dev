import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { playerWithHand, opponent } from "../_shared/players";

// The victory point badge lists where each point came from.
test("tooltip: victory point breakdown", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand({ victory_points: 3 }), opponent],
    table: { revealed_objectives: ["amass_wealth", "develop"], scored_objectives: { gallery_seat: ["amass_wealth", "develop"] } },
  });
  await hoverShot(page, testInfo, "04-vp-breakdown", page.getByTestId("player-vp").nth(1));
});
