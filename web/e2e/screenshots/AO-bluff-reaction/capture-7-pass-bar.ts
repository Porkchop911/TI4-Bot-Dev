import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { GAME_ID } from "../_shared/mockGame";
import { PROTOCOL_VERSION } from "../../../src/protocol/types";
import { intentState, openBluffGame } from "./_bluff";

// The bluffer's own screen during the stall: the usual status for a seat being waited for, and a
// bar to end the wait early. Nobody else sees the bar.
test("the bluffer: Pass ends the wait early", async ({ page }, testInfo) => {
  const sent: string[] = [];
  const { send, snapshot } = await openBluffGame(page, {
    onClientMessage: (message) => sent.push(message.type),
  });
  send(intentState({ triggers: ["space_combat"], locked_until_round: 3, holding: true }));
  send({
    type: "turn_status",
    protocol_version: PROTOCOL_VERSION,
    game_id: GAME_ID,
    game_version: snapshot.game_version,
    status: { kind: "waiting_for_decision", seat: snapshot.view.players[0].id, phase: "action", round: 2, stage: "Waiting for player" },
  });
  const bar = page.getByTestId("bluff-hold-bar");
  await expect(bar).toBeVisible();
  await shot(page, testInfo, "8-bluffer-pass-bar", { of: page.locator(".game-header"), pad: 8 });

  await page.getByTestId("bluff-pass").click();
  await expect.poll(() => sent.includes("pass_reaction_hold")).toBe(true);
  // The server then ends the hold; the bar goes away.
  send(intentState({ triggers: ["space_combat"], locked_until_round: 3, holding: false }));
  await expect(bar).toHaveCount(0);
});
