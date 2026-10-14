import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { turnMenu, prefix } from "./scene";
import { opponent, playerWithHand } from "../_shared/players";

const status = {
  seat: opponent.id,
  requested_by: opponent.id,
  turns_back: 1,
  redo_count: 1,
  stage: "new_turn",
  original_decisions: 120,
  rewound_to: 101,
  turn_complete: false,
  handoff_len: null,
  outcome: null,
  can_control: false,
};

for (const size of [{ width: 1440, height: 900 }, { width: 390, height: 844 }]) {
  test(`the redo bar can be hidden ${size.width}`, async ({ page }, testInfo) => {
    await page.setViewportSize(size);
    await page.route("**/turn-redo**", (route) => route.fulfill({ json: { status } }));
    await openMockedGame(page, { players: [playerWithHand({}), opponent], choice: turnMenu });
    const bar = page.getByTestId("turn-redo-bar");
    await expect(bar).toBeVisible({ timeout: 10_000 });
    await shot(page, testInfo, `${prefix}-redo-bar-${size.width}`);
    if (prefix === "before") return;
    await page.getByTestId("turn-redo-hide").click();
    await expect(bar).toHaveCount(0);
    await shot(page, testInfo, `after-redo-bar-hidden-${size.width}`);
  });
}
