import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting, push, save } from "./prep";

// Auto mode, but the plan does not validate: the real UI opens at once with "Needs review".
test("secondary prep: auto mode falls back to the real UI", async ({ page }, testInfo) => {
  const game = await openWaiting(page, { name: "Technology", card: "pok7technology" });
  await page.getByTestId("secondary-prep-mode-auto").click();
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("secondary-yes-btn").click();
  await save(page);
  push(game, {
    version: 42,
    history: { cursor: 41, redo_count: 0, generation: 0 },
    choice: {
      prompt: "spend a strategy token and 4 resources to research",
      subtype: "strategy_secondary",
      options: [{ id: "no", kind: "strategy", label: "decline" }],
      details: { kind: "strategy_secondary", card: "pok7technology", played_by: "other_seat", tokens_left: 0, costs_token: true },
    },
  });
  await page.getByTestId("secondary-needs-review").waitFor();
  await shot(page, testInfo, "12-auto-invalid-ui-opens");
});
