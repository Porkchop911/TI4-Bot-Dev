import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent } from "../_shared/players";
import { openCornerGame, cornerCrop } from "../G-corner-notifications/corner";
import { PROTOCOL_VERSION } from "../../../src/protocol/types";

// A window whose only card is on Never is declined without asking; the seat's next state update
// carries a note and the corner toast says why no dialog appeared.
test("action cards: skipped window toast", async ({ page }, testInfo) => {
  const game = await openCornerGame(page, { players: [playerWithHand(), opponent] });
  const { events: _events, ...update } = game.snapshot;
  game.send({
    ...update,
    type: "state_update",
    protocol_version: PROTOCOL_VERSION,
    game_version: game.snapshot.game_version + 1,
    reaction_modes: { Sabotage: "never" },
    auto_resolved: [
      {
        id: "auto-skip-1",
        prompt: "Reaction window (Sabotage)",
        selected: "Pass",
        reason: "you set Sabotage to never offer",
      },
    ],
  });
  await page.getByTestId("corner-toast").waitFor();
  await shot(page, testInfo, "6-skipped-window-toast", cornerCrop(page));
});
