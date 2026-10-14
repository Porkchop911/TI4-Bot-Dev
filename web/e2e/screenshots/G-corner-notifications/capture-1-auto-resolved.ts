import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { cornerCrop } from "./corner";

// The only legal option of a decision was taken for the player; the real mounted toast says what
// and why. (The server marks such an option `auto_resolved`.)
test("notifications: one auto-selected choice", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: {
      prompt: "Strategy Card",
      context: { subtype: "draft_strategy_card" },
      options: [{ id: "pok7warfare", label: "Warfare", kind: "strategy_card", auto_resolved: true } as never],
    },
  });
  await page.getByTestId("corner-toast").waitFor();
  await shot(page, testInfo, "1-auto-resolved", cornerCrop(page));
});
