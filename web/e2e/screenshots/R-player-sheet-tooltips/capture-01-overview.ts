import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";

// The economy row at rest: compact TG / Comm and the bare token numbers.
test("player sheet: economy and token line at rest", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand({ trade_goods: 4, commodities: 2 }), opponent] });
  const row = page.getByTestId("player-command-tokens").first().locator("xpath=../../..");
  await row.scrollIntoViewIfNeeded();
  await page.waitForTimeout(300);
  const box = (await row.boundingBox())!;
  await page.screenshot({
    path: `${testInfo.file.replace(/[^/]+$/, "")}out/01-overview.png`,
    animations: "disabled",
    clip: { x: Math.max(0, box.x - 24), y: Math.max(0, box.y - 60), width: box.width + 48, height: box.height + 100 },
  });
});
