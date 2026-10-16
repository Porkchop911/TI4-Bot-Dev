import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openBotLobby } from "./scene";

// Host view of a 4-seat lobby on a desktop window. TI4_SHOT_PREFIX=before is how the committed
// "before" shots were taken (on the base commit, unified-latest-2026-10-08); the default "after"
// regenerates the current layout. On the base commit the lobby ignores `bot_kinds` and `bot`, so
// the same mocked server shows what the host had then: no way to add a bot without a password.
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";
const before = prefix === "before";

test("host: empty lobby", async ({ page }, testInfo) => {
  await openBotLobby(page, { botKinds: ["random"], guest: true });
  await expect(page.getByTestId("lobby-container")).toBeVisible();
  if (!before) {
    await expect(page.getByTestId("add-random-bot-button")).toBeVisible();
    await expect(page.getByTestId("fill-random-bots-button")).toBeVisible();
  }
  await shot(page, testInfo, `${prefix}-1-host-empty`);
});

test("host: one random bot added", async ({ page }, testInfo) => {
  await openBotLobby(page, { botKinds: ["random"], guest: true, initialBots: before ? 1 : 0 });
  if (!before) {
    await page.getByTestId("add-random-bot-button").click();
    await expect(page.getByTestId("bot-badge-3")).toHaveText("Random bot");
  }
  await shot(page, testInfo, `${prefix}-2-one-bot`);
});

test("host: every open seat filled", async ({ page }, testInfo) => {
  await openBotLobby(page, { botKinds: ["random"], guest: true, initialBots: before ? 2 : 0 });
  if (!before) {
    await page.getByTestId("fill-random-bots-button").click();
    await expect(page.getByTestId("bot-badge-4")).toBeVisible();
    // Nothing is left to add, so the buttons are gone and the host can start once Blair is ready.
    await expect(page.getByTestId("add-random-bot-button")).toHaveCount(0);
  }
  await shot(page, testInfo, `${prefix}-3-filled`);
});

test("host: remove a bot", async ({ page }, testInfo) => {
  await openBotLobby(page, { botKinds: ["random"], guest: true, initialBots: 2 });
  if (!before) {
    await page.getByRole("button", { name: "Remove bot at position 3" }).click();
    await expect(page.getByTestId("add-random-bot-button")).toBeVisible();
    await expect(page.getByTestId("bot-badge-3")).toHaveCount(0);
  } else {
    await page.getByTestId("remove-bot-button-3").click();
  }
  await shot(page, testInfo, `${prefix}-4-bot-removed`);
});
