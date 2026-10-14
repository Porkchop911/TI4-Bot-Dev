import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { thirdPlayer } from "../G-corner-notifications/corner";
import { PRIMARY, playedEvent, push } from "../AH-secondary-prep-real-ui/prep";
import { prefix, record } from "./shared";

// Bug 1: the chip said Prepared, then went back to "Prepare your secondary" the moment the engine
// asked ANOTHER follower (view.active_player is the seat being asked, not the primary), and the saved
// plan was dropped. The mock below asks the third seat while the viewer's plan is saved.
test("prepared chip survives another seat being asked", async ({ page }, testInfo) => {
  const game = await openMockedGame(page, {
    players: [
      playerWithHand({ strategy_cards: ["pok8imperial"], strategic_tokens: 2, trade_goods: 5 }),
      { ...opponent, strategy_cards: ["pok7technology", "pok1leadership"] },
      thirdPlayer,
    ],
    events: [playedEvent("Technology")],
    view: { active_player: PRIMARY },
    turnStatus: { kind: "waiting_for_decision", seat: PRIMARY, phase: "action", round: 2, stage: "Waiting for player" },
  });
  push(game, { history: { cursor: 40, redo_count: 0, generation: 0 }, version: 41 });
  await page.waitForTimeout(150);
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("secondary-yes-btn").click();
  await page.locator('[data-testid^="tech-card-"][data-selectable="true"]').first().click();
  await page.getByTestId("confirm-research-btn").click();
  const chip = page.getByTestId("secondary-prep-chip");
  await chip.filter({ hasText: "Prepared" }).waitFor();
  // The engine now asks the third seat (a follower before the viewer): the same action goes on.
  const { events: _events, ...base } = game.snapshot;
  game.send({
    ...base,
    type: "state_update",
    game_version: 42,
    history: { cursor: 41, redo_count: 0, generation: 0 },
    view: { ...base.view, active_player: thirdPlayer.id },
    turn_status: { kind: "waiting_for_decision", seat: thirdPlayer.id, phase: "action", round: 2, stage: "strategy" },
  });
  await page.waitForTimeout(500);
  const text = ((await chip.count()) ? await chip.first().innerText() : "(no chip)").replace(/\s+/g, " ");
  record("chip-after-third-seat-is-asked", text);
  await shot(page, testInfo, `${prefix}-6-prepared-chip-other-seat-asked`);
});
