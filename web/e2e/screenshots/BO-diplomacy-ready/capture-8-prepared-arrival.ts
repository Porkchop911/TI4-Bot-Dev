import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting, push, secondaryQuestion } from "../AH-secondary-prep-real-ui/prep";
import { board, EXHAUSTED, prefix } from "./scene";

// The prepared pair when the real question opens: both planets already picked, Confirm is the
// player's. Before: the plan answered the first question with its first planet and the second
// question with the next one, one after the other.
test("diplomacy ready: prepared pair at arrival", async ({ page }, testInfo) => {
  const game = await openWaiting(page, { name: "Diplomacy", card: "pok2diplomacy", board });
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("secondary-yes-btn").click();
  if (prefix === "before") {
    await page.getByTestId("planet-jord").click();
    await page.getByTestId("confirm-planet-btn").click();
    // The second question opens with the first planet gone from the list.
    await page.getByTestId("planet-chip-jord").waitFor({ state: "detached" });
    await page.getByTestId("planet-tarmann").click();
    await page.getByTestId("planet-selection-action").waitFor();
    await page.getByTestId("confirm-planet-btn").click();
  } else {
    await page.getByTestId("planet-jord").click();
    await page.getByTestId("ready-planet-chip-tarmann").click();
    await page.getByTestId("ready-bar-confirm").click();
  }
  await page.getByTestId("secondary-prep-chip").waitFor();
  // The secondary window opens, is followed, and the engine asks which planets to ready.
  push(game, {
    version: 42,
    history: { cursor: 41, redo_count: 0, generation: 0 },
    choice: secondaryQuestion("pok2diplomacy", "spend a strategy token to ready two planets", "ready"),
  });
  await page.waitForTimeout(200);
  push(game, {
    version: 43,
    history: { cursor: 42, redo_count: 0, generation: 0 },
    choice: {
      prompt: "ready which planet",
      subtype: "ready_planet",
      options: EXHAUSTED.map((planet) => ({ id: planet, kind: "ready", label: `ready ${planet}`, payload: { planet } })),
    },
  });
  // The old UI shows its prepared-answer bar over the question; whichever is up is the shot.
  await page
    .getByTestId(prefix === "before" ? "planet-selection-bar" : "ready-planets-bar")
    .waitFor({ timeout: prefix === "before" ? 2_000 : 10_000 })
    .catch(() => undefined);
  await page.waitForTimeout(300);
  await shot(page, testInfo, `${prefix}-8-prepared-arrival`);
});
