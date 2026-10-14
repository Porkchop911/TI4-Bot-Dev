import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openBotLobby } from "./scene";

// The host of a 4-seat lobby on a server that offers both in-process bot kinds.
const BOTH = ["random", "smart"];

test("host: both bot kinds offered", async ({ page }, testInfo) => {
  await openBotLobby(page, { botKinds: BOTH, guest: true });
  await expect(page.getByTestId("add-smart-bot-button")).toBeVisible();
  await expect(page.getByTestId("fill-smart-bots-button")).toBeVisible();
  await expect(page.getByTestId("add-random-bot-button")).toBeVisible();
  await expect(page.getByTestId("random-bot-actions")).toContainText("Rule bots claim nearby planets");
  await shot(page, testInfo, "1-host-both-kinds");
});

test("host: one rule bot added", async ({ page }, testInfo) => {
  await openBotLobby(page, { botKinds: BOTH, guest: true });
  await page.getByTestId("add-smart-bot-button").click();
  await expect(page.getByTestId("bot-badge-3")).toHaveText("Rule bot");
  await expect(page.getByText("Rule bot 1")).toBeVisible();
  await shot(page, testInfo, "2-one-smart-bot");
});

test("host: a random and a rule bot side by side", async ({ page }, testInfo) => {
  await openBotLobby(page, { botKinds: BOTH, guest: true });
  await page.getByTestId("add-random-bot-button").click();
  await expect(page.getByTestId("bot-badge-3")).toHaveText("Random bot");
  await page.getByTestId("fill-smart-bots-button").click();
  await expect(page.getByTestId("bot-badge-4")).toHaveText("Rule bot");
  // No seat left: the add buttons are gone.
  await expect(page.getByTestId("add-smart-bot-button")).toHaveCount(0);
  await shot(page, testInfo, "3-mixed-table");
});

test("older server: random only, no smart buttons", async ({ page }, testInfo) => {
  await openBotLobby(page, { botKinds: ["random"], guest: true });
  await expect(page.getByTestId("add-random-bot-button")).toBeVisible();
  await expect(page.getByTestId("add-smart-bot-button")).toHaveCount(0);
  await shot(page, testInfo, "4-random-only-server");
});
