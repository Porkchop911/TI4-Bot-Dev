import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { acceptingScript, leadershipWindow, openScene, prefix, prepareAndSave, pushChoice } from "./scene";

// Jord was exhausted meanwhile (an earlier follower or the primary used it). After: Needs review
// names the planet and what Auto-pay would use instead, which is already selected in the panel for
// one click. Before: nothing noticed; the bar simply offered the Auto-pay payment as if it were the plan.
test("leadership prep: a prepared planet is gone on arrival", async ({ page }, testInfo) => {
  const game = await openScene(page, acceptingScript);
  await prepareAndSave(page);
  pushChoice(
    game,
    42,
    leadershipWindow(
      [
        { id: "lodor", worth: 3 },
        { id: "quann", worth: 1 },
      ],
      2,
    ),
  );
  await page.getByTestId(prefix === "before" ? "secondary-prepared-bar" : "secondary-needs-review").waitFor();
  await page.waitForTimeout(300);
  await shot(page, testInfo, `${prefix}-4-stale-payment`);
});
