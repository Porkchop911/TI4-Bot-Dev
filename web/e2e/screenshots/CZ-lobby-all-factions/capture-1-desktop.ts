import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openFactionLobby } from "./scene";

test("create dialog: four players by default", async ({ page }, testInfo) => {
  await page.route("**/api/health", (route) => route.fulfill({ json: { status: "ok" } }));
  await page.goto("/");
  await expect(page.getByLabel("Players")).toHaveValue("4");
  await shot(page, testInfo, "1-create-dialog");
});

test("host: the full picker, 24 factions grouped, searchable", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 1250 });
  await openFactionLobby(page, { chosen: { 1: "sol" }, bots: ["hacan"] });
  await page.getByTestId("seat-faction-picker-1-button").click();
  await expect(page.getByRole("option")).toHaveCount(25);
  await expect(page.getByTestId("seat-faction-picker-1-option-sol")).toHaveAttribute("aria-selected", "true");
  await expect(page.getByTestId("seat-faction-picker-1-option-hacan")).toHaveAttribute("aria-disabled", "true");
  await shot(page, testInfo, "2-full-picker");
  await page.getByTestId("seat-faction-picker-1-search").fill("na");
  await shot(page, testInfo, "3-picker-search");
});

test("guest: chooses their own faction, others stay text", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 1250 });
  await openFactionLobby(page, { viewer: "guest_1", chosen: { 1: "sol", 3: "titans" }, thirdPerson: true });
  await expect(page.getByTestId("seat-faction-picker-1-button")).toHaveCount(0);
  await expect(page.getByTestId("seat-faction-1")).toContainText("Federation of Sol");
  await page.getByTestId("seat-faction-picker-2-button").click();
  await expect(page.getByTestId("seat-faction-picker-2-option-sol")).toHaveAttribute("aria-disabled", "true");
  await expect(page.getByTestId("seat-faction-picker-2-option-sol")).toContainText("taken by Alex");
  await page.getByTestId("seat-faction-picker-2-option-mahact").click();
  await expect(page.getByTestId("seat-faction-picker-2-button")).toContainText("Mahact");
  await shot(page, testInfo, "4-guest-chose-own");
});

test("host: chooses for another person", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 1250 });
  await openFactionLobby(page, { chosen: { 1: "sol", 3: "titans" }, thirdPerson: true, bots: ["nomad"] });
  await page.getByTestId("seat-faction-picker-2-button").click();
  await expect(page.getByTestId("seat-faction-picker-2-option-titans")).toContainText("taken by Casey");
  await shot(page, testInfo, "5-host-picks-for-guest-open");
  await page.getByTestId("seat-faction-picker-2-option-yssaril").click();
  await expect(page.getByTestId("seat-faction-picker-2-button")).toContainText("Yssaril");
  await shot(page, testInfo, "6-host-picked-for-guest");
});

test("duplicate that the view did not show yet is refused", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 1250 });
  await openFactionLobby(page, { hiddenHolder: { faction: "nekro", name: "Casey" } });
  await page.getByTestId("seat-faction-picker-1-button").click();
  await page.getByTestId("seat-faction-picker-1-option-nekro").click();
  await expect(page.getByTestId("lobby-error")).toContainText("already taken by Casey");
  await shot(page, testInfo, "7-duplicate-refused");
});
