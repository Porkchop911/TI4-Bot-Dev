import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test, type Locator } from "@playwright/test";

// The shell of a game on a Pixel 9 in portrait. Needs the engine: `scripts/build-wasm.sh`.
const WASM = fileURLToPath(new URL("../src/session/ti4.wasm", import.meta.url));
test.skip(!existsSync(WASM), "the engine is not built: run web2/scripts/build-wasm.sh");

const SHOTS = process.env.WEB2_SHOTS ?? "shots/phone/shell";

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
  // A row opens to its details in groups.
  const strategic = rows.filter({ hasText: "Strategic · Trade" });
  await strategic.getByRole("button").click();
  await expect(strategic.getByRole("region", { name: "Secondaries" })).toContainText("Followed");
  const tactical = rows.filter({ hasText: "Tactical · " }).first();
  await tactical.getByRole("button").click();
  await expect(tactical.getByRole("region", { name: "Activation" })).toBeVisible();
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

test("phone: the Log pane has the strategic action that the other seats follow, read-only, its parts in groups", async ({
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
  const now = log.getByRole("article", { name: "Current action" });
  await expect(now.getByRole("heading").first()).toHaveText("Leadership");
  const seats = now.getByRole("button", { name: "Seats" });
  await expect(seats).toHaveAttribute("aria-expanded", "true");
  await expect(now).toContainText("✓ Resolved");
  await readOnly(log);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    page.viewportSize()!.width,
  );
  await page.screenshot({ path: `${SHOTS}/strategic.png`, animations: "disabled" });
  await seats.click();
  await expect(seats).toHaveAttribute("aria-expanded", "false");
  await expect(now).not.toContainText("✓ Resolved");
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
