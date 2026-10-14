import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { showTitleTooltip } from "../_shared/tooltip";
import { playerWithHand, opponent } from "../_shared/players";

// The toggle explains itself on hover (native title text, drawn by the helper).
test("action cards: toggle tooltip", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent] });
  const toggle = page.getByTestId("reaction-inspect-mode-sabo1");
  await showTitleTooltip(page, toggle);
  await shot(page, testInfo, "3-toggle-tooltip", { of: page.getByTestId("player-card").first(), pad: 120 });
});
