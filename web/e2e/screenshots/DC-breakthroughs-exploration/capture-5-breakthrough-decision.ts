import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { board, players, actor } from "./_fixtures";

// A decision a breakthrough raised prints the card: Yssaril's Deepgloom Executable asks its owner.
test("decision raised by a breakthrough shows the card text", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const { list, state } = players();
  await openMockedGame(page, {
    board,
    players: list,
    state,
    choice: {
      prompt: "Deepgloom Executable: allow Player 2 to use Scheming",
      context: { subtype: "yssaril_scheming", source: { Content: "yssarilbt" }, phase: "ACTION", round: 2, optional: true },
      options: [
        { id: "allow", kind: "breakthrough", label: "allow Scheming" },
        { id: "decline", kind: "decline", label: "decline" },
      ],
      player: actor,
    },
  });
  const card = page.getByTestId("decision-breakthrough");
  await card.waitFor();
  await expect(card).toContainText("Deepgloom Executable");
  await shot(page, testInfo, "decision-breakthrough");
});

// The turn menu's Thunder's Edge entry names the breakthrough its slice grants.
test("expedition entry names the breakthrough it grants", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const { list, state } = players(null);
  await openMockedGame(page, {
    board,
    players: list,
    state,
    choice: {
      prompt: "action phase",
      details: { kind: "turn_menu", closing: false, tokens: { tactic: 3, fleet: 3, strategy: 2 }, strategy_cards: [], partners: [] },
      options: [
        { id: "tactical", kind: "action", label: "Tactical action" },
        { id: "pass", kind: "action", label: "Pass" },
        { id: "component|expedition|trade_goods", kind: "component", label: "Thunder's Edge expedition: spend 5 trade goods" },
      ],
    },
  });
  await page.getByTestId("turn-action-bar").waitFor();
  await page.getByTestId("turn-bar-components").click();
  const entry = page.getByTestId("turn-bar-item-component|expedition|trade_goods");
  await entry.hover();
  await expect(page.getByTestId("turn-bar-info")).toContainText("Grants your breakthrough, Bellum Gloriosum");
  await shot(page, testInfo, "expedition-note");
});
