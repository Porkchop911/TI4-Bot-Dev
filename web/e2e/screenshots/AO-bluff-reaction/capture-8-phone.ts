import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { GAME_ID, openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { PROTOCOL_VERSION } from "../../../src/protocol/types";
import { intentState, openBluffGame, openSelector, ownCard } from "./_bluff";

const PHONE = { width: 390, height: 844 };
// A reaction wait names no seat: the same status for a real window and a bluff hold.
const reactionWait = { kind: "waiting_for_reactions" as const, phase: "action", round: 2 };
const waiting = {
  kind: "waiting_for_decision" as const,
  seat: playerWithHand().id,
  phase: "action",
  round: 2,
  stage: "Waiting for player",
};

test("phone: the selector on the player mat", async ({ page }, testInfo) => {
  await openBluffGame(page, PHONE);
  // On a phone the player sheet is a drawer behind the Players button.
  await page.getByRole("button", { name: "Players", exact: true }).click();
  await openSelector(page);
  await page.getByTestId("bluff-trigger-ground_combat").check();
  await shot(page, testInfo, "9-phone-selector", { of: ownCard(page), pad: 6 });
});

test("phone: another client's banner during a stall", async ({ page }, testInfo) => {
  await page.setViewportSize(PHONE);
  const { send } = await openMockedGame(page, {
    seat: opponent.id,
    players: [playerWithHand(), opponent],
    turnStatus: { kind: "active_turn", player: waiting.seat, phase: "action", round: 2 },
  });
  const bar = page.getByTestId("turn-status-bar");
  await expect(bar).toContainText("Active Turn");
  send({ type: "turn_status", protocol_version: PROTOCOL_VERSION, game_id: GAME_ID, game_version: 40, status: reactionWait });
  await expect(bar).toContainText("Waiting for");
  await shot(page, testInfo, "10-phone-banner", { of: bar, pad: 6 });
});

test("phone: the bluffer's Pass bar", async ({ page }, testInfo) => {
  const { send, snapshot } = await openBluffGame(page, PHONE);
  send(intentState({ triggers: ["space_combat"], locked_until_round: 3, holding: true }));
  send({
    type: "turn_status",
    protocol_version: PROTOCOL_VERSION,
    game_id: GAME_ID,
    game_version: snapshot.game_version,
    status: { ...reactionWait },
  });
  await expect(page.getByTestId("bluff-hold-bar")).toBeVisible();
  await shot(page, testInfo, "11-phone-pass-bar", { of: page.locator(".game-header"), pad: 6 });
});
