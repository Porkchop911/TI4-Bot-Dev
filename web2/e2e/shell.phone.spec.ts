import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test, type Locator } from "@playwright/test";

// The shell of a game on a Pixel 9 in portrait. Needs the engine: `scripts/build-wasm.sh`.
const WASM = fileURLToPath(new URL("../src/session/ti4.wasm", import.meta.url));
test.skip(!existsSync(WASM), "the engine is not built: run web2/scripts/build-wasm.sh");

const SHOTS = process.env.WEB2_SHOTS ?? "shots/phone/shell";

/** Nothing in the Log is a control. */
const readOnly = async (log: Locator) => {
  await expect(log.locator("input, select, textarea, button")).toHaveCount(0);
};

test("phone: the panes are Map, Players and Log, and the open step is a sheet over the board", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/?local=3&players=8&humans=1");
  const panes = page.getByRole("navigation", { name: "Panes" });
  const pane = (name: string) => panes.getByRole("button", { name });
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const sheet = page.getByRole("region", { name: "Open step" });
  const row = (name: RegExp) => sheet.locator("#step-panel").getByRole("button", { name });
  const width = page.viewportSize()!.width;
  const fits = async () =>
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);

  await expect(panes.getByRole("button")).toHaveText(["Map", "Players", "Log"]);
  // The game opens on the pane of its choice: every choice but a seat is on the map.
  await expect(pane("Map")).toHaveAttribute("aria-pressed", "true");
  const board = page.getByRole("region", { name: "Galaxy board" });
  await expect(board).toBeVisible();
  // The strategy card is chosen in the sheet, and the main button under the pane sends it.
  await row(/^8\. Imperial$/).click();
  await expect(send).toBeInViewport({ ratio: 1 });
  await send.click();
  // The action picker is the next step, in the same sheet over the board.
  await expect(row(/^Activate a system$/)).toBeVisible();
  await expect(pane("Map")).toHaveAttribute("aria-pressed", "true");
  const boardBox = (await board.boundingBox())!;
  const sheetBox = (await sheet.boundingBox())!;
  // The sheet is at the bottom of the board, and the board stays in view above it.
  expect(sheetBox.y + sheetBox.height).toBeCloseTo(boardBox.y + boardBox.height, 0);
  expect(sheetBox.height).toBeLessThanOrEqual(boardBox.height * 0.6 + 1);
  await fits();
  await page.screenshot({ path: `${SHOTS}/picker.png`, animations: "disabled" });
  // A staged action has the main button in view.
  await row(/^8 /).first().click();
  await expect(send).toBeInViewport({ ratio: 1 });
  await expect(send).toBeEnabled();
  await fits();
  await page.screenshot({ path: `${SHOTS}/staged.png`, animations: "disabled" });

  // The Log has a row for each turn of the other seats, before the first turn of the viewer.
  await pane("Log").click();
  const log = page.getByRole("region", { name: "Log" });
  const rows = log.getByRole("list", { name: "Turns" }).getByRole("article");
  await expect(rows).toHaveCount(7);
  // The staged action of the picker is the viewer's decision: the Log has no copy of it, and no
  // second "?".
  await expect(log.getByRole("article", { name: "Current action" })).toHaveCount(0);
  await expect(log).not.toContainText("Activate a system");
  await expect(log.getByRole("button", { name: /^Rules/ })).toHaveCount(0);
  // A row is large: the seat, the action and its outcome, with no dice.
  await expect(rows.filter({ hasText: "Strategic · Trade" })).toContainText("followed");
  for (const turn of await rows.all()) {
    expect((await turn.boundingBox())!.height).toBeGreaterThan(40);
  }
  await expect(log.locator("[aria-label=Rolls], .die")).toHaveCount(0);
  expect(await log.innerText()).not.toMatch(/\b[a-z]+_[a-z0-9_]+\b/);
  await readOnly(log);
  await fits();
  await page.screenshot({ path: `${SHOTS}/log.png`, animations: "disabled" });
  await pane("Players").click();
  await expect(page.getByRole("region", { name: "Players" })).toBeVisible();
  await fits();
  expect(errors).toEqual([]);
});

test("phone: with no decision of the viewer open there is no step sheet", async ({ page }) => {
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
  await expect(page.locator("header strong")).toHaveText("Waiting for Hacan");
  // The board is free; the footer stays under the pane.
  await expect(sheet).toHaveCount(0);
  await expect(page.locator("footer").getByRole("button", { name: "Undo" })).toBeInViewport();
  await page.screenshot({ path: `${SHOTS}/waiting.png`, animations: "disabled" });
  expect(errors).toEqual([]);
});

test("phone: the Log pane has the strategic action that the other seats follow as its live row", async ({
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

  await page
    .getByRole("navigation", { name: "Panes" })
    .getByRole("button", { name: "Log" })
    .click();
  const log = page.getByRole("region", { name: "Log" });
  // The action is the turn of the viewer that is played now: the last row, with "▶" and "You".
  const live = log.getByRole("list", { name: "Turns" }).getByRole("article").last();
  await expect(live).toContainText("▶");
  await expect(live).toContainText("You");
  await expect(live).toContainText("Strategic · Leadership");
  await readOnly(log);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    page.viewportSize()!.width,
  );
  await page.screenshot({ path: `${SHOTS}/strategic.png`, animations: "disabled" });
  expect(errors).toEqual([]);
});

test("phone: while the viewer decides a secondary, the sheet on the map has its row and its choice, and the Log pane the turn of Sol", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  // Sol and Hacan are played here: Sol plays Leadership, then Hacan is asked its secondary.
  await page.goto("/?scenario=strategy_leadership&humans=3");
  const sheet = page.getByRole("region", { name: "Open step" });
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  await sheet
    .locator("#step-panel")
    .getByRole("button", { name: /Leadership/ })
    .first()
    .click();
  await send.click();
  for (let at = 0; at < 3; at++) {
    await sheet.getByRole("button", { name: "Add tactic pool token" }).click();
  }
  await send.click();
  await expect(sheet.locator("#step-panel")).toContainText("Strategic action · Sol");
  await expect(sheet.getByRole("button", { name: "Add bought token" })).toBeVisible();
  await expect(sheet).not.toContainText("Resolved");
  await expect(sheet).not.toContainText("Then: ");
  await page.screenshot({ path: `${SHOTS}/secondary.png`, animations: "disabled" });

  await page
    .getByRole("navigation", { name: "Panes" })
    .getByRole("button", { name: "Log" })
    .click();
  const log = page.getByRole("region", { name: "Log" });
  const live = log.getByRole("list", { name: "Turns" }).getByRole("article").last();
  await expect(live).toContainText("▶");
  await expect(live).toContainText("Strategic · Leadership");
  await expect(log).not.toContainText("Command pools");
  await readOnly(log);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    page.viewportSize()!.width,
  );
  await page.screenshot({ path: `${SHOTS}/secondary-log.png`, animations: "disabled" });
  expect(errors).toEqual([]);
});

test("phone: the step sheet minimizes to its heading line, the main button under the pane still sends, and a new question opens it again", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/?local=3&players=8&humans=1");
  const board = page.getByRole("region", { name: "Galaxy board" });
  const sheet = page.getByRole("region", { name: "Open step" });
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const row = (name: RegExp) => sheet.locator("#step-panel").getByRole("button", { name });
  const minimize = sheet.getByRole("button", { name: "Minimize" });
  const restore = sheet.getByRole("button", { name: "Restore" });

  await row(/^8\. Imperial$/).click();
  await minimize.click();
  // One line at the bottom of the board.
  await expect(sheet.locator("#step-panel")).toHaveCount(0);
  const boardBox = (await board.boundingBox())!;
  const small = (await sheet.boundingBox())!;
  expect(small.height).toBeLessThan(60);
  expect(small.y + small.height).toBeCloseTo(boardBox.y + boardBox.height, 0);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    page.viewportSize()!.width,
  );
  await page.screenshot({ path: `${SHOTS}/minimized.png`, animations: "disabled" });
  await restore.click();
  await expect(row(/^8\. Imperial$/)).toHaveAttribute("aria-pressed", "true");
  await minimize.click();
  // The main button under the pane sends; the next question opens the sheet again.
  await expect(send).toBeInViewport({ ratio: 1 });
  await send.click();
  await expect(row(/^Activate a system$/)).toBeVisible();
  await expect(minimize).toBeVisible();
  expect(errors).toEqual([]);
});
