import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test, type Locator } from "@playwright/test";

// The shell of a game on desktop. Needs the engine: `scripts/build-wasm.sh`.
const WASM = fileURLToPath(new URL("../src/session/ti4.wasm", import.meta.url));
test.skip(!existsSync(WASM), "the engine is not built: run web2/scripts/build-wasm.sh");
test.use({ viewport: { width: 1920, height: 1080 } });

const SHOTS = process.env.WEB2_SHOTS ?? "shots/desktop/shell";

/**
 * Nothing in the Log is a control: every button that it has changes what is in view (a group, a
 * rule text, the rolls of a battle).
 */
const readOnly = async (log: Locator) => {
  await expect(log.locator("input, select, textarea")).toHaveCount(0);
  await expect(log.locator('button:not([aria-expanded]):not([aria-label="Rolls"] *)')).toHaveCount(
    0,
  );
};

test("desktop: the right column is the players and the log, and the open step is a sheet over the board at the width of a phone", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/?local=3&players=8&humans=1");
  const board = page.getByRole("region", { name: "Galaxy board" });
  const sheet = page.getByRole("region", { name: "Open step" });
  const send = sheet.locator('footer button[aria-keyshortcuts="Enter"]');
  const heading = sheet.locator("#step-panel").getByRole("heading").first();
  const row = (name: RegExp) => sheet.locator("#step-panel").getByRole("button", { name });
  const fits = async () =>
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(1920);

  // There are no panes: the column has the player table with eight seats, and the log under it.
  await expect(page.getByRole("navigation", { name: "Panes" })).toHaveCount(0);
  const players = page.getByRole("region", { name: "Players" });
  await expect(players.getByRole("listitem")).toHaveCount(8);
  const log = page.getByRole("region", { name: "Log" });
  await expect(log).toContainText("No turns since your last turn");
  const playersBox = (await players.boundingBox())!;
  const logBox = (await log.boundingBox())!;
  expect(logBox.x).toBeCloseTo(playersBox.x, 0);
  expect(logBox.y).toBeGreaterThanOrEqual(playersBox.y + playersBox.height - 1);
  // The log fills the column to the bottom of the screen.
  expect(logBox.y + logBox.height).toBeCloseTo(1080, 0);

  // The strategy card is chosen in the sheet, and its main button is in the sheet too.
  await expect(sheet).toHaveAttribute("aria-label", "Open step");
  await row(/^8\. Imperial$/).click();
  await expect(send).toBeInViewport({ ratio: 1 });
  await send.click();
  // The action picker is the next step, in the same sheet: at the bottom right of the board, at
  // the width of a phone, and the board stays in view above it.
  await expect(row(/^Activate a system$/)).toBeVisible();
  const boardBox = (await board.boundingBox())!;
  const sheetBox = (await sheet.boundingBox())!;
  expect(sheetBox.width).toBeCloseTo(412, 0);
  expect(sheetBox.x + sheetBox.width).toBeLessThanOrEqual(boardBox.x + boardBox.width);
  expect(boardBox.x + boardBox.width - (sheetBox.x + sheetBox.width)).toBeLessThan(20);
  expect(boardBox.y + boardBox.height - (sheetBox.y + sheetBox.height)).toBeLessThan(20);
  expect(sheetBox.height).toBeLessThanOrEqual(boardBox.height * 0.65 + 1);
  // The sheet is in the phone layout: a row has no column of its shortcut key.
  await expect(sheet.locator("#step-panel kbd")).not.toHaveCount(0);
  for (const key of await sheet.locator("#step-panel kbd").all()) {
    await expect(key).toBeHidden();
  }
  await fits();
  await page.screenshot({ path: `${SHOTS}/picker.png`, animations: "disabled" });

  // The shortcut keys of the open step work as before: they read the step, not the screen.
  await page.keyboard.press("t");
  await expect(heading).toHaveText("Activation");
  await expect(page.locator("g.system.target:not(.plain)")).not.toHaveCount(0);
  await page.locator("g.system.target:not(.plain)").first().click();
  await expect(send).toBeEnabled();
  await send.click();
  await expect(heading).toHaveText("Movement");
  await fits();
  await page.screenshot({ path: `${SHOTS}/movement.png`, animations: "disabled" });
  // The Log has the action too, read-only: each step is a line with its state, and the step in
  // view is a group that is open.
  const now = log.getByRole("article", { name: "Current action" });
  await expect(now.getByRole("heading").first()).toHaveText(/^Tactical action · /);
  const movement = now.getByRole("button", { name: /Movement/ });
  await expect(movement).toHaveAttribute("aria-expanded", "true");
  await expect(now).toContainText("✓Activation");
  await readOnly(log);
  await movement.click();
  await expect(movement).toHaveAttribute("aria-expanded", "false");
  await movement.click();
  // An open fleet has the room of the step sheet, and its check mark gives it back.
  await page.locator("g.system").filter({ hasText: "#01" }).click();
  const fleet = page.getByRole("group", { name: /^Move from / });
  await expect(fleet).toBeVisible();
  await expect(sheet).toHaveCount(0);
  await page.screenshot({ path: `${SHOTS}/fleet.png`, animations: "disabled" });
  await fleet.getByRole("button", { name: "Done with this system" }).click();
  await expect(heading).toHaveText("Movement");
  expect(errors).toEqual([]);
});

test("desktop: with no decision of the viewer open there is no step sheet, the footer keeps its buttons, and the Log has the action that is played", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  // The other seats wait for a step, so that the page shows the game while another seat decides.
  await page.addInitScript(() => localStorage.setItem("ti4.local.stepping", "1"));
  await page.goto("/?local=3&players=8&humans=1");
  const sheet = page.getByRole("region", { name: "Open step" });
  await sheet
    .locator("#step-panel")
    .getByRole("button", { name: /^8\. Imperial$/ })
    .click();
  await page.locator('footer button[aria-keyshortcuts="Enter"]').click();
  // Whom the game waits for is the status of the toolbar; the board is free.
  await expect(page.locator("header strong")).toHaveText("Waiting for Hacan");
  await expect(sheet).toHaveCount(0);
  await expect(page.locator("#step-panel")).toHaveCount(0);
  // The footer alone stays over the board while it has a button: here the undo and the step.
  await expect(page.locator("footer").getByRole("button", { name: "Undo" })).toBeVisible();
  // The Log has the action that is played now, read-only.
  const log = page.getByRole("region", { name: "Log" });
  const now = log.getByRole("article", { name: "Current action" });
  await expect(now.getByRole("heading").first()).toHaveText("Waiting");
  await expect(now).toContainText("Hacan decides next.");
  await readOnly(log);
  await page.screenshot({ path: `${SHOTS}/waiting.png`, animations: "disabled" });

  expect(errors).toEqual([]);
});

test("desktop: while the other seats follow a strategy card of the viewer, the Log has it, read-only, its parts in groups", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  // The other seats wait for a step: their secondaries wait, and the strategic action stays.
  await page.addInitScript(() => localStorage.setItem("ti4.local.stepping", "1"));
  await page.goto("/?local=3&players=8&humans=1");
  const sheet = page.getByRole("region", { name: "Open step" });
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const row = (name: RegExp) => sheet.locator("#step-panel").getByRole("button", { name });
  await row(/^1\. Leadership$/).click();
  await send.click();
  // The other seats choose their cards; then the viewer has the first turn.
  const step = page.locator("footer").getByRole("button", { name: "Step" });
  const leadership = row(/^1 Leadership/);
  await expect(async () => {
    if (!(await leadership.isVisible())) {
      await step.click({ timeout: 500 });
    }
    await expect(leadership).toBeVisible({ timeout: 300 });
  }).toPass({ timeout: 8_000 });
  await leadership.click();
  await send.click();
  // The primary: three command tokens, in the tactic pool.
  for (let at = 0; at < 3; at++) {
    await sheet.getByRole("button", { name: /^Add / }).first().click();
  }
  await send.click();
  await expect(page.locator("header strong")).toHaveText(/^Waiting for /);

  const log = page.getByRole("region", { name: "Log" });
  const now = log.getByRole("article", { name: "Current action" });
  await expect(now.getByRole("heading").first()).toHaveText("Leadership");
  // The seats are a group that is open, with the seats that wait in one line.
  const seats = now.getByRole("button", { name: "Seats" });
  await expect(seats).toHaveAttribute("aria-expanded", "true");
  await expect(now).toContainText("Then: ");
  await readOnly(log);
  await page.screenshot({ path: `${SHOTS}/strategic.png`, animations: "disabled" });
  await expect(now).toContainText("✓ Resolved");
  await seats.click();
  await expect(seats).toHaveAttribute("aria-expanded", "false");
  await expect(now).not.toContainText("✓ Resolved");
  await seats.click();
  await expect(now).toContainText("✓ Resolved");
  expect(errors).toEqual([]);
});
