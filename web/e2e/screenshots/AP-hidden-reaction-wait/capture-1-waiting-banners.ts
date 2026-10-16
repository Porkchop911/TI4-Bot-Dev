import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent } from "../_shared/players";
import { openMockedGame, GAME_ID } from "../_shared/mockGame";
import { PROTOCOL_VERSION, type PublicTurnStatus } from "../../../src/protocol/types";

// TI4_SHOT_PREFIX=before is how the committed "before" shots were taken (on the base commit
// bluff-reaction-intent-2026-10-08, where a reaction wait still named the asked seat); the
// default "after" regenerates the current layout. The mocked status is whatever that commit's
// server sends other viewers while a reaction window waits on the seat holding a card.
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";
const holder = playerWithHand().id;
const named: PublicTurnStatus = {
  kind: "waiting_for_decision",
  seat: holder,
  phase: "action",
  round: 2,
  stage: "Waiting for player",
};
const reactionWait: PublicTurnStatus =
  prefix === "before" ? named : { kind: "waiting_for_reactions", phase: "action", round: 2 };
const viewport = { width: 1440, height: 900 };
const players = [playerWithHand(), opponent];

test("another seat: reaction wait, bluff hold and an ordinary wait", async ({ browser }, testInfo) => {
  const real = await (await browser.newContext({ viewport, colorScheme: "dark" })).newPage();
  await openMockedGame(real, { seat: opponent.id, players, turnStatus: reactionWait });
  const realBar = real.getByTestId("turn-status-bar");
  await realBar.waitFor();

  // A bluff hold: the page first shows the ordinary turn, then the server pushes the hold status.
  const bluff = await (await browser.newContext({ viewport, colorScheme: "dark" })).newPage();
  const { send } = await openMockedGame(bluff, {
    seat: opponent.id,
    players,
    turnStatus: { kind: "active_turn", player: holder, phase: "action", round: 2 },
  });
  const bluffBar = bluff.getByTestId("turn-status-bar");
  await expect(bluffBar).not.toContainText("Waiting for");
  send({ type: "turn_status", protocol_version: PROTOCOL_VERSION, game_id: GAME_ID, game_version: 40, status: reactionWait });
  await expect(bluffBar).toContainText("Waiting for");
  expect(await bluffBar.innerHTML()).toBe(await realBar.innerHTML());
  if (prefix !== "before") {
    await expect(realBar.getByTestId("turn-status-banner")).toHaveText("Waiting for reactions");
    expect(await realBar.innerText()).not.toContain(holder);
  }

  // An ordinary question (not a reaction window) keeps naming the seat.
  const ordinary = await (await browser.newContext({ viewport, colorScheme: "dark" })).newPage();
  await openMockedGame(ordinary, { seat: opponent.id, players, turnStatus: named });
  const ordinaryBar = ordinary.getByTestId("turn-status-bar");
  await ordinaryBar.waitFor();
  await expect(ordinaryBar.getByTestId("turn-status-banner")).toContainText("Waiting for");

  await shot(real, testInfo, `${prefix}-1-reaction-wait-other-seat`, { of: realBar, pad: 12 });
  await shot(bluff, testInfo, `${prefix}-2-bluff-hold-other-seat`, { of: bluffBar, pad: 12 });
  await shot(ordinary, testInfo, `${prefix}-3-ordinary-wait`, { of: ordinaryBar, pad: 12 });
  await shot(real, testInfo, `${prefix}-4-reaction-wait-full-view`);

  // The asked seat itself still gets its own prompt status.
  const asked = await (await browser.newContext({ viewport, colorScheme: "dark" })).newPage();
  await openMockedGame(asked, { seat: holder, players, turnStatus: named });
  const askedBar = asked.getByTestId("turn-status-bar");
  await expect(askedBar.getByTestId("turn-status-banner")).toContainText("YOUR TURN");
  await shot(asked, testInfo, `${prefix}-5-asked-seat`, { of: askedBar, pad: 12 });
  for (const page of [real, bluff, ordinary, asked]) await page.context().close();
});
