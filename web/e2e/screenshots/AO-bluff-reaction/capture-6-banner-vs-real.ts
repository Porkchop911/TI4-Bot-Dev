import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent } from "../_shared/players";
import { openMockedGame, GAME_ID } from "../_shared/mockGame";
import { PROTOCOL_VERSION } from "../../../src/protocol/types";

// A reaction wait names no seat: the same status for a real window and a bluff hold.
const reactionWait = { kind: "waiting_for_reactions" as const, phase: "action", round: 2 };
const waiting = {
  kind: "waiting_for_decision" as const,
  seat: playerWithHand().id,
  phase: "action",
  round: 2,
  stage: "Waiting for player",
};

// The other player's screen. Left: the game really is waiting for the seat (it holds a card that
// fits). Right: the seat bluffs, so the server puts up the very same status for the stall. The
// test fails if the two ever differ by a character or a node.
test("another client: real wait next to a bluff hold", async ({ browser }, testInfo) => {
  const players = [playerWithHand(), opponent];
  const viewport = { width: 1440, height: 900 };

  const real = await (await browser.newContext({ viewport, colorScheme: "dark" })).newPage();
  await openMockedGame(real, { seat: opponent.id, players, turnStatus: reactionWait });
  const realBar = real.getByTestId("turn-status-bar");
  await realBar.waitFor();

  const bluff = await (await browser.newContext({ viewport, colorScheme: "dark" })).newPage();
  const { send } = await openMockedGame(bluff, {
    seat: opponent.id,
    players,
    turnStatus: { kind: "active_turn", player: waiting.seat, phase: "action", round: 2 },
  });
  const bluffBar = bluff.getByTestId("turn-status-bar");
  await expect(bluffBar).not.toContainText("Waiting for");
  // What the server sends other viewers when a declared window opens for a seat it does not ask.
  send({
    type: "turn_status",
    protocol_version: PROTOCOL_VERSION,
    game_id: GAME_ID,
    game_version: 40,
    status: reactionWait,
  });
  await expect(bluffBar).toContainText("Waiting for");

  // Byte for byte the same banner.
  expect(await bluffBar.innerHTML()).toBe(await realBar.innerHTML());
  expect(await bluffBar.innerText()).toBe(await realBar.innerText());
  expect(await bluff.getByTestId("bluff-hold-bar").count()).toBe(0);

  await shot(real, testInfo, "6-banner-real-wait", { of: realBar, pad: 12 });
  await shot(bluff, testInfo, "7-banner-bluff-hold", { of: bluffBar, pad: 12 });
  await real.context().close();
  await bluff.context().close();
});
