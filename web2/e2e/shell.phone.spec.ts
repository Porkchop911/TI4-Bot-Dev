import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";

// The shell of a game on a Pixel 9 in portrait. Needs the engine: `scripts/build-wasm.sh`.
const WASM = fileURLToPath(new URL("../src/session/ti4.wasm", import.meta.url));
test.skip(!existsSync(WASM), "the engine is not built: run web2/scripts/build-wasm.sh");

const SHOTS = process.env.WEB2_SHOTS ?? "shots/phone/shell";

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

  await pane("Log").click();
  await expect(page.getByRole("region", { name: "Log" })).toContainText(
    "No turns since your last turn",
  );
  await fits();
  await pane("Players").click();
  await expect(page.getByRole("region", { name: "Players" })).toBeVisible();
  await fits();
  expect(errors).toEqual([]);
});
