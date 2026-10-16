import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";

// The shell of a game on desktop. Needs the engine: `scripts/build-wasm.sh`.
const WASM = fileURLToPath(new URL("../src/session/ti4.wasm", import.meta.url));
test.skip(!existsSync(WASM), "the engine is not built: run web2/scripts/build-wasm.sh");
test.use({ viewport: { width: 1920, height: 1080 } });

const SHOTS = process.env.WEB2_SHOTS ?? "shots/desktop/shell";

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
