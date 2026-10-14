import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { actor, galleryBoard, galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

const invasionBoard = {
  ...galleryBoard,
  invasion: {
    system_id: "18",
    invasion_seq: 1,
    invader: actor,
    phase: "landing" as const,
    planets: ["jord"],
    current_planet: null,
    defender: null,
    ground_round: 0,
    odds_context: {},
  },
};

// Landing troops: the best planet is pre-selected, and the badge says why.
test("tooltip: invasion default target badge", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    board: invasionBoard,
    choice: galleryDecision("tactical invasion"),
  });
  await hoverShot(page, testInfo, "17-invasion-default", page.getByLabel("default target").first(), { native: true, pad: 90 });
});
