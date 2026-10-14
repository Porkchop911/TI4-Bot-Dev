import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openBotLobby } from "./scene";

// Who does not get the buttons: another player, and a host on a server that predates random bots.
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";
const before = prefix === "before";

test("guest sees the bots but no host controls", async ({ page }, testInfo) => {
  await openBotLobby(page, { botKinds: ["random"], viewer: "guest_1", initialBots: 2 });
  await expect(page.getByTestId("lobby-container")).toBeVisible();
  await expect(page.getByTestId("add-random-bot-button")).toHaveCount(0);
  if (!before) await expect(page.getByTestId("bot-badge-3")).toBeVisible();
  await shot(page, testInfo, `${prefix}-7-guest`);
});

test("older server: no bot kinds, no random bot buttons", async ({ page }, testInfo) => {
  await openBotLobby(page, { guest: true });
  await expect(page.getByTestId("lobby-container")).toBeVisible();
  await expect(page.getByTestId("add-random-bot-button")).toHaveCount(0);
  await shot(page, testInfo, `${prefix}-8-older-server`);
});

test("server with the MLP service: both ways to add a bot", async ({ page }, testInfo) => {
  await openBotLobby(page, { botKinds: ["random", "mlp"], mlp: true, guest: true });
  if (!before) await expect(page.getByTestId("add-random-bot-button")).toBeVisible();
  await expect(page.getByTestId("add-bot-button-3")).toBeVisible();
  await shot(page, testInfo, `${prefix}-9-with-mlp`);
});
