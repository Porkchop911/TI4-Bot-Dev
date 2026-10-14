import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { playerWithHand, opponent } from "../_shared/players";

// A strategy card chip shows its initiative and both abilities.
test("tooltip: strategy card", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand({ strategy_cards: ["pok3politics"], exhausted_strategy_cards: [] }), opponent] });
  await hoverShot(page, testInfo, "10-strategy-card", page.getByTestId("strategy-card-badge-pok3politics").first(), { native: true });
});
