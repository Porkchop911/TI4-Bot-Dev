import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";
import { shot } from "../_shared/shot";

// After playing Bribery the holder is asked one question: how many trade goods to spend.
test("bribery asks one count", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openMockedGame(page, {
    phase: "agenda",
    players: [playerWithHand(), opponent],
    choice: {
      prompt: "Bribery: how many trade goods to spend?",
      context: { subtype: "generic_selection", optional: false, source: { Card: "bribery" } },
      options: [0, 1, 2, 3].map((n) => ({ id: String(n), label: `spend ${n} trade goods`, kind: "count" })) as never,
    },
  });
  await page.waitForTimeout(800);
  await shot(page, testInfo, "2-spend");
});
