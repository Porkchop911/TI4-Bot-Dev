import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openFactionLobby } from "./scene";

test("host: picker closed, Random by default, only the rule bot offered", async ({ page }, testInfo) => {
  await openFactionLobby(page);
  await expect(page.getByTestId("new-bot-faction-button")).toContainText("Random");
  await expect(page.getByTestId("add-smart-bot-button")).toHaveText("Add bot");
  await expect(page.getByTestId("add-random-bot-button")).toHaveCount(0);
  await expect(page.getByTestId("fill-random-bots-button")).toHaveCount(0);
  await shot(page, testInfo, "1-picker-closed");
});

test("host: picker open, a held faction is disabled", async ({ page }, testInfo) => {
  await openFactionLobby(page, { bots: ["hacan"] });
  await page.getByTestId("new-bot-faction-button").click();
  await expect(page.getByRole("option")).toHaveCount(7);
  await expect(page.getByTestId("new-bot-faction-option-hacan")).toHaveAttribute("aria-disabled", "true");
  await shot(page, testInfo, "2-picker-open");
});

test("host: bots added with chosen factions, the rest left on Random", async ({ page }, testInfo) => {
  await openFactionLobby(page);
  await page.getByTestId("new-bot-faction-button").click();
  await page.getByTestId("new-bot-faction-option-xxcha").click();
  await page.getByTestId("add-smart-bot-button").click();
  await expect(page.getByTestId("seat-faction-picker-3-button")).toContainText("Xxcha");
  await page.getByTestId("add-smart-bot-button").click();
  await expect(page.getByTestId("seat-faction-picker-4-button")).toContainText("Random");
  await page.getByTestId("new-bot-faction-button").click();
  await page.getByTestId("new-bot-faction-option-l1z1x").click();
  await page.getByTestId("add-smart-bot-button").click();
  await expect(page.getByTestId("seat-faction-picker-5-button")).toContainText("L1Z1X");
  await shot(page, testInfo, "3-bots-with-factions");
});

test("host: changing the faction of a bot seat after adding it", async ({ page }, testInfo) => {
  await openFactionLobby(page, { bots: ["xxcha", null, "l1z1x"] });
  await page.getByTestId("seat-faction-picker-4-button").click();
  await expect(page.getByTestId("seat-faction-picker-4-option-xxcha")).toHaveAttribute("aria-disabled", "true");
  await shot(page, testInfo, "4-change-bot-faction-open");
  await page.getByTestId("seat-faction-picker-4-option-jolnar").click();
  await expect(page.getByTestId("seat-faction-picker-4-button")).toContainText("Jol-Nar");
  await shot(page, testInfo, "5-change-bot-faction-done");
});

test("host: a duplicate the lobby view did not show yet is refused with the server's reason", async ({ page }, testInfo) => {
  await openFactionLobby(page, { hiddenHolder: { faction: "sol", name: "Rule bot 9" } });
  await page.getByTestId("new-bot-faction-button").click();
  await page.getByTestId("new-bot-faction-option-sol").click();
  await page.getByTestId("add-smart-bot-button").click();
  await expect(page.getByTestId("lobby-error")).toContainText("already taken by Rule bot 9");
  await expect(page.getByTestId("bot-badge-3")).toHaveCount(0);
  await shot(page, testInfo, "6-duplicate-refused");
});

test("guest: sees the factions as text, with no pickers", async ({ page }, testInfo) => {
  await openFactionLobby(page, { viewer: "guest_1", bots: ["sol", null, "letnev"] });
  await expect(page.getByTestId("seat-faction-3")).toContainText("Federation of Sol");
  await expect(page.getByTestId("seat-faction-4")).toContainText("Random faction");
  await expect(page.getByTestId("seat-faction-picker-3-button")).toHaveCount(0);
  await expect(page.getByTestId("new-bot-faction-button")).toHaveCount(0);
  await shot(page, testInfo, "7-guest");
});
