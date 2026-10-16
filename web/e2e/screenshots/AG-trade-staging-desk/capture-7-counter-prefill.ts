import { test } from "@playwright/test";
import { PROTOCOL_VERSION } from "../../../src/protocol/types";
import { openMockedGame } from "../_shared/mockGame";
import { answerChoice, PARTNER, tradePlayers, proposeOptions, click, shotDesk } from "./_trade";

// Counter-offer: choosing it submits "counter"; the engine then asks this seat to propose from the
// other chair, and the desk opens with the partner's deal staged as this seat would propose it.
test("trade staging desk: counter-offer pre-filled", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1100, height: 1250 });
  const game = await openMockedGame(page, { players: tradePlayers, choice: answerChoice as never });
  await page.getByTestId("answer-opt-counter").waitFor();
  await click(page, "answer-opt-counter");
  const { events: _events, ...update } = game.snapshot;
  game.send({
    ...update,
    type: "state_update",
    protocol_version: PROTOCOL_VERSION,
    game_version: game.snapshot.game_version + 1,
    pending_choice: {
      nonce: "trade-counter",
      choice: {
        player: game.snapshot.viewer.role === "player" ? game.snapshot.viewer.seat : "",
        prompt: "transaction with Jolnar",
        context: { subtype: "propose_transaction", target: { Player: PARTNER } },
        // This seat's own list: it holds no way to give 3 commodities for 2 goods plus a note.
        options: proposeOptions,
      },
    },
  });
  await page.getByTestId("counter-prefill-note").waitFor();
  await shotDesk(page, testInfo, "7-counter-prefill");
});
