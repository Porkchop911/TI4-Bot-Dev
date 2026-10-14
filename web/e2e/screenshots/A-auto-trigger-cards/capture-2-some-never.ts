import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";

// Two cards switched to Never. The clicks go to the (mocked) server as set_reaction_mode and the
// switches are drawn from the state update that comes back, as in a live game.
test("action cards: some set to never", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent] });
  const toggle = (id: string) => page.getByTestId(`reaction-inspect-mode-${id}`);
  await toggle("direct_hit").click();
  await expect(toggle("direct_hit")).toHaveAttribute("data-reaction-mode", "never");
  await toggle("skilled_retreat").click();
  await expect(toggle("skilled_retreat")).toHaveAttribute("data-reaction-mode", "never");
  await expect(toggle("sabo1")).toHaveAttribute("data-reaction-mode", "always");
  const card = page.getByTestId("player-card").first();
  await shot(page, testInfo, "2-some-never", { of: card, pad: 6 });
});
