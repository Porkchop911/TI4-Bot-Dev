import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test, type Locator, type Page } from "@playwright/test";

// The shell of a game on a Pixel 9 in portrait. Needs the engine: `scripts/build-wasm.sh`.
const WASM = fileURLToPath(new URL("../src/session/ti4.wasm", import.meta.url));
test.skip(!existsSync(WASM), "the engine is not built: run web2/scripts/build-wasm.sh");

const SHOTS = process.env.WEB2_SHOTS ?? "shots/phone/shell";

/** Nothing in the Log is a control but the rows of the turns and the phases that open. */
const readOnly = async (log: Locator) => {
  await expect(log.locator("input, select, textarea, button:not([aria-pressed])")).toHaveCount(0);
  for (const row of await log.locator("button[aria-pressed]").all()) {
    await expect(row).toHaveAttribute(
      "aria-label",
      /· (Tactical|Strategic|Component)|^(Strategy|Status|Agenda) phase · Round \d+$/,
    );
  }
};

/** The board has one sheet over it: the decision, Watch or an opened turn, never two. */
async function oneSheet(page: Page, name: "Open step" | "Watch" | "Turn") {
  const board = page.getByRole("region", { name: "Galaxy board" });
  for (const other of ["Open step", "Watch", "Turn"]) {
    await expect(board.getByRole("region", { name: other, exact: true })).toHaveCount(
      other === name ? 1 : 0,
    );
  }
  await expect(board.getByRole("button", { name: "Restore" })).toHaveCount(0);
}

test("phone: the panes are Map, Players and Log, and the open step is a sheet over the board", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  // The check of the changes of the board (`checkBoard`, in development) says a gap here.
  page.on("console", (message) => {
    if (message.type() === "error" && message.text().startsWith("[board history]")) {
      errors.push(message.text());
    }
  });
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
  const rows = log
    .getByRole("list", { name: "Turns" })
    .locator("li > article, li > button[aria-label^='Turn ']");
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
  // The check of the changes of the board (`checkBoard`, in development) says a gap here.
  page.on("console", (message) => {
    if (message.type() === "error" && message.text().startsWith("[board history]")) {
      errors.push(message.text());
    }
  });
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
  // The game waits for no decision of the viewer: a line over the board says what is played now,
  // and a tap on it opens Watch, the one sheet over the board, with no bar of a decision.
  const board = page.getByRole("region", { name: "Galaxy board" });
  const line = board.getByRole("button", { name: /^Played now: .*Waiting for Hacan/ });
  await expect(line).toBeVisible();
  await page.screenshot({ path: `${SHOTS}/live-line.png`, animations: "disabled" });
  await line.click();
  await oneSheet(page, "Watch");
  await expect(line).toHaveCount(0);
  await expect(page.getByRole("region", { name: "Your decision" })).toHaveCount(0);
  await expect(page.locator("header")).not.toContainText("History");
  await page.screenshot({ path: `${SHOTS}/watch-phase.png`, animations: "disabled" });
  expect(errors).toEqual([]);
});

test("phone: the Log pane has the strategic action that the other seats follow as its live row", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  // The check of the changes of the board (`checkBoard`, in development) says a gap here.
  page.on("console", (message) => {
    if (message.type() === "error" && message.text().startsWith("[board history]")) {
      errors.push(message.text());
    }
  });
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
  // The action is the turn of the viewer that is played now: the first row, with "▶" and "You".
  // A strategic turn opens its action: its row is a button.
  const live = log
    .getByRole("list", { name: "Turns" })
    .locator("li > article, li > button")
    .first();
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

test("phone: while the viewer decides a secondary, the sheet on the map has its row and its choice, the Log pane the turn of Sol, and its live row opens Watch", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  // The check of the changes of the board (`checkBoard`, in development) says a gap here.
  page.on("console", (message) => {
    if (message.type() === "error" && message.text().startsWith("[board history]")) {
      errors.push(message.text());
    }
  });
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
  await expect(sheet).toContainText("Strategic action · Sol");
  await expect(sheet.getByRole("button", { name: "Add bought token" })).toBeVisible();
  await expect(sheet).not.toContainText("Resolved");
  await expect(sheet).not.toContainText("Then: ");
  await page.screenshot({ path: `${SHOTS}/secondary.png`, animations: "disabled" });

  await page
    .getByRole("navigation", { name: "Panes" })
    .getByRole("button", { name: "Log" })
    .click();
  const log = page.getByRole("region", { name: "Log" });
  // A strategic turn opens its action: its row is a button.
  const live = log
    .getByRole("list", { name: "Turns" })
    .locator("li > article, li > button")
    .first();
  await expect(live).toContainText("▶");
  await expect(live).toContainText("Strategic · Leadership");
  await expect(log).not.toContainText("Command pools");
  await readOnly(log);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    page.viewportSize()!.width,
  );
  await page.screenshot({ path: `${SHOTS}/secondary-log.png`, animations: "disabled" });

  // The live row of Sol opens Watch on Map: the turn that is played now, read-only, over the live
  // board. It is the one sheet over the board: the decision is its bar in the place of the footer.
  await live.click();
  const panes = page.getByRole("navigation", { name: "Panes" });
  await expect(panes.getByRole("button", { name: "Map" })).toHaveAttribute("aria-pressed", "true");
  const watch = page.getByRole("region", { name: "Watch" });
  await expect(watch).toContainText("Played now");
  await expect(watch.locator("ul[aria-label=Seats] > li").filter({ hasText: "You" })).toContainText(
    "▶ Deciding",
  );
  const board = page.getByRole("region", { name: "Galaxy board" });
  const line = board.getByRole("button", { name: /^Played now: / });
  await expect(board.locator("[data-past]")).toHaveCount(0);
  await expect(page.locator("header")).not.toContainText("History");
  await oneSheet(page, "Watch");
  const waiting = page.getByRole("region", { name: "Your decision" });
  await expect(waiting).toContainText("Your decision is waiting: ");
  await expect(send).toHaveCount(0);
  await expect(line).toHaveCount(0);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    page.viewportSize()!.width,
  );
  expect(await watch.evaluate((element) => element.scrollWidth - element.clientWidth)).toBe(0);
  await page.screenshot({ path: `${SHOTS}/watch.png`, animations: "disabled" });
  // "Back" closes Watch: the decision is back. The action waits for the viewer, so no line says it.
  await waiting.getByRole("button", { name: "Back" }).click();
  await expect(watch).toHaveCount(0);
  await expect(sheet.getByRole("button", { name: "Add bought token" })).toBeVisible();
  await oneSheet(page, "Open step");
  await expect(line).toHaveCount(0);
  await page.screenshot({ path: `${SHOTS}/waits-for-you.png`, animations: "disabled" });
  expect(errors).toEqual([]);
});

test("phone: the step sheet minimizes to its heading line, the main button under the pane still sends, and a new question opens it again", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  // The check of the changes of the board (`checkBoard`, in development) says a gap here.
  page.on("console", (message) => {
    if (message.type() === "error" && message.text().startsWith("[board history]")) {
      errors.push(message.text());
    }
  });
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

test("phone: a tactical turn of the Log opens on Map over the board, read-only, in the history mode with the decision as a bar that waits", async ({
  page,
}) => {
  test.setTimeout(20_000);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error" && message.text().startsWith("[board history]")) {
      errors.push(message.text());
    }
  });
  // Sol's space combat at #29 is under way, and Sol is the viewer.
  await page.goto("/?scenario=ongoing_combat&humans=1");
  const panes = page.getByRole("navigation", { name: "Panes" });
  const pane = (name: string) => panes.getByRole("button", { name });
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const sheet = page.getByRole("region", { name: "Open step" });
  const log = page.getByRole("region", { name: "Log" });
  const row = log.getByRole("button", { name: /· Tactical/ });
  const width = page.viewportSize()!.width;
  // The viewer answers until the first round of the combat is rolled.
  for (let answer = 0; answer < 12; answer += 1) {
    await pane("Log").click();
    if (/destroyed 3/.test(await row.innerText())) {
      break;
    }
    await pane("Map").click();
    if (await send.isEnabled()) {
      await send.click();
    } else {
      await sheet.locator("#step-panel button:not([aria-label=Minimize])").first().click();
    }
  }
  await expect(row).toContainText("destroyed 3");

  // A tap on the row goes to Map, with the turn over the board.
  await row.click();
  await expect(pane("Map")).toHaveAttribute("aria-pressed", "true");
  const turn = page.getByRole("region", { name: "Turn" });
  await expect(turn).toBeVisible();
  const tabs = turn.getByRole("tablist", { name: "Tactical action steps" }).getByRole("tab");
  await expect(tabs).toHaveText([/Activation/, /Movement/, /Space combat/]);
  // The heading row is the context of the action and comes first; the tab names the step.
  const heading = turn.getByRole("heading").first();
  await expect(heading).toHaveText(/^Tactical action · /);
  await expect(turn.getByRole("heading", { name: "Space combat" })).toHaveCount(0);
  expect((await heading.boundingBox())!.y).toBeLessThan((await tabs.first().boundingBox())!.y);
  await expect(turn.getByRole("img", { name: /^Rolled \d+: (hit|miss)$/ }).first()).toBeVisible();
  await expect(turn.locator("input, select, textarea")).toHaveCount(0);
  // The history mode: the toolbar says which point of the game the screen shows, with the way
  // back, and the stamp on the board which step.
  const header = page.locator("header");
  await expect(header).toContainText("History");
  await expect(header).toContainText(/Round 1 · Turn \d+ · /);
  await expect(header.getByRole("navigation", { name: "Workspace" })).toHaveCount(0);
  await expect(header.getByRole("button", { name: "Back to now" })).toBeVisible();
  const stamp = page
    .getByRole("region", { name: "Galaxy board" })
    .getByRole("status", { name: "Board shows" });
  await expect(stamp).toHaveText(/after Space combat$/);
  await expect(stamp).toBeInViewport();
  // The stamp covers no system of the turn: the board frames the marked system above it.
  const marked = page
    .getByRole("region", { name: "Galaxy board" })
    .locator('[aria-label*="the open window or turn is about this system"]');
  const [system, mark] = [(await marked.boundingBox())!, (await stamp.boundingBox())!];
  expect(system.y + system.height).toBeLessThanOrEqual(mark.y);
  // "‹" and "›" are in the toolbar, around the turn, in one line; the sheet has no line for them.
  const turns = header.getByRole("navigation", { name: "Turns of the Log" });
  await expect(turns.getByRole("button", { name: "Older turn" })).toBeVisible();
  await expect(turns.getByRole("button", { name: "Newer turn" })).toBeDisabled();
  expect((await turns.boundingBox())!.height).toBeLessThan(40);
  await expect(turn.getByRole("navigation", { name: "Turns of the Log" })).toHaveCount(0);
  await expect(turn.getByRole("button", { name: "Close" })).toHaveCount(0);
  // No control of the live game: the decision is a bar in the place of the footer.
  await expect(send).toHaveCount(0);
  await expect(page.getByText("This commits to the live game")).toHaveCount(0);
  const waiting = page.getByRole("region", { name: "Your decision" });
  await expect(waiting).toContainText("Your decision is waiting: ");
  await expect(waiting).toBeInViewport();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
  expect(await turn.evaluate((element) => element.scrollWidth - element.clientWidth)).toBe(0);
  await page.screenshot({ path: `${SHOTS}/turn.png`, animations: "disabled" });

  // One sheet over the board at a time: the inspector of a system has the place of the turn, the
  // board stays the past one, and its "×" gives the turn back.
  const board = page.getByRole("region", { name: "Galaxy board" });
  await page.locator("g.system").filter({ hasText: "#29" }).first().click();
  const close = board.getByRole("button", { name: "Close system details" });
  await expect(close).toBeVisible();
  await expect(turn).toHaveCount(0);
  await expect(board.locator("[data-past]")).toHaveCount(1);
  await expect(waiting).toBeInViewport();
  await page.screenshot({ path: `${SHOTS}/turn-inspector.png`, animations: "disabled" });
  await close.click();
  await expect(turn).toBeVisible();

  // "▾" minimizes the turn to one line; the screen stays in the history mode.
  await turn.getByRole("button", { name: "Minimize" }).click();
  await expect(tabs).toHaveCount(0);
  await expect(turn.getByRole("button", { name: "Restore" })).toContainText("Space combat");
  await expect(header).toContainText("History");
  await page.screenshot({ path: `${SHOTS}/turn-minimized.png`, animations: "disabled" });
  await turn.getByRole("button", { name: "Restore" }).click();
  await expect(tabs).toHaveCount(3);

  // "Back to now" ends the mode, and the step sheet is back; so does "Back" of the bar.
  await header.getByRole("button", { name: "Back to now" }).click();
  await expect(turn).toHaveCount(0);
  await expect(sheet).toBeVisible();
  // The live step has the same order: the context, then the tabs, then the step.
  const live = sheet.getByRole("heading").first();
  await expect(live).toHaveText(/^Tactical action · /);
  expect((await live.boundingBox())!.y).toBeLessThan(
    (await sheet.getByRole("tablist", { name: "Tactical action steps" }).boundingBox())!.y,
  );
  await page.screenshot({ path: `${SHOTS}/live-step.png`, animations: "disabled" });
  // The inspector has the place of the step sheet too, and gives it back. (#29 is under the
  // sheet: the click is sent to it.)
  await page.locator("g.system").filter({ hasText: "#29" }).first().dispatchEvent("click");
  await expect(close).toBeVisible();
  await expect(sheet).toHaveCount(0);
  await page.screenshot({ path: `${SHOTS}/live-inspector.png`, animations: "disabled" });
  await close.click();
  await expect(sheet).toBeVisible();
  await expect(send).toBeVisible();
  await pane("Log").click();
  await row.click();
  await expect(turn).toBeVisible();
  await waiting.getByRole("button", { name: "Back" }).click();
  await expect(turn).toHaveCount(0);
  await expect(sheet).toBeVisible();
  expect(errors).toEqual([]);
});

test("phone: a strategic turn of the Log opens on Map, with its seats, and the map brings the system of Diplomacy into view above the sheet", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error" && message.text().startsWith("[board history]")) {
      errors.push(message.text());
    }
  });
  await page.goto("/?local=3&players=8&humans=1");
  const panes = page.getByRole("navigation", { name: "Panes" });
  const pane = (name: string) => panes.getByRole("button", { name });
  const sheet = page.getByRole("region", { name: "Open step" });
  const row = (name: RegExp) => sheet.locator("#step-panel").getByRole("button", { name });
  // The viewer has the last card: the seven other seats play their first turn before it.
  await row(/^8\. Imperial$/).click();
  await page.locator('footer button[aria-keyshortcuts="Enter"]').click();
  await expect(row(/^Activate a system$/)).toBeVisible();

  await pane("Log").click();
  const log = page.getByRole("region", { name: "Log" });
  await readOnly(log);
  await log.getByRole("button", { name: /· Strategic · Diplomacy/ }).click();
  // A tap on the row goes to Map, with the turn over the board.
  await expect(pane("Map")).toHaveAttribute("aria-pressed", "true");
  const turn = page.getByRole("region", { name: "Turn" });
  await expect(turn.getByRole("heading", { name: "Diplomacy" })).toBeVisible();
  const rows = turn.locator("ul[aria-label=Seats] > li");
  await expect(rows.first()).toContainText("✓ Resolved");
  // Every other seat is a row.
  await expect(rows).toHaveCount(8);
  // The system of Diplomacy is marked, and in view above the sheet.
  const board = page.getByRole("region", { name: "Galaxy board" });
  const marked = board.locator('[aria-label*="the open window or turn is about this system"]');
  await expect(marked).toHaveCount(1);
  const tile = (await marked.boundingBox())!;
  const top = (await turn.boundingBox())!.y;
  const area = (await board.boundingBox())!;
  expect(tile.y).toBeGreaterThanOrEqual(area.y);
  expect(tile.y + tile.height).toBeLessThanOrEqual(top + 1);
  expect(tile.x).toBeGreaterThanOrEqual(area.x - 1);
  expect(tile.x + tile.width).toBeLessThanOrEqual(area.x + area.width + 1);
  const width = page.viewportSize()!.width;
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
  expect(await turn.evaluate((element) => element.scrollWidth - element.clientWidth)).toBe(0);
  await page.screenshot({ path: `${SHOTS}/strategic-turn.png`, animations: "disabled" });
  await page.getByRole("button", { name: "Back to now" }).click();
  await expect(turn).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("phone: a strategic turn with a tactical action opens on Map to its card, then the steps of its tactical action", async ({
  page,
}) => {
  test.setTimeout(15_000);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error" && message.text().startsWith("[board history]")) {
      errors.push(message.text());
    }
  });
  // Hacan is played here: Sol plays Warfare of Thunder's Edge, a tactical action in any system.
  await page.goto("/?scenario=strategy_warfare_te&humans=2");
  await page
    .getByRole("navigation", { name: "Panes" })
    .getByRole("button", { name: "Log" })
    .click();
  await page
    .getByRole("region", { name: "Log" })
    .getByRole("button", { name: /· Strategic · Warfare/ })
    .click();
  // Sol's turn is played now: its row opens Watch, on Map.
  const turn = page.getByRole("region", { name: "Watch" });
  const tabs = turn.getByRole("tab");
  await expect(tabs.first()).toHaveText(/Warfare/);
  await expect(tabs.nth(1)).toHaveText(/Activation/);
  await expect(turn.getByRole("heading", { name: "Warfare" })).toHaveCount(0);
  await expect(turn.locator("ul[aria-label=Seats] > li").first()).not.toContainText("Activated");
  await tabs.nth(1).click();
  await expect(tabs.nth(1)).toHaveAttribute("aria-selected", "true");
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    page.viewportSize()!.width,
  );
  await page.screenshot({ path: `${SHOTS}/warfare-turn.png`, animations: "disabled" });
  expect(errors).toEqual([]);
});

test("phone: a phase of the Log opens on Map with what the seats did in it", async ({ page }) => {
  test.setTimeout(10_000);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error" && message.text().startsWith("[board history]")) {
      errors.push(message.text());
    }
  });
  await page.goto("/?local=3&players=8&humans=1");
  const panes = page.getByRole("navigation", { name: "Panes" });
  const pane = (name: string) => panes.getByRole("button", { name });
  const sheet = page.getByRole("region", { name: "Open step" });
  const row = (name: RegExp) => sheet.locator("#step-panel").getByRole("button", { name });
  await row(/^8\. Imperial$/).click();
  await page.locator('footer button[aria-keyshortcuts="Enter"]').click();
  await expect(row(/^Activate a system$/)).toBeVisible();

  await pane("Log").click();
  const log = page.getByRole("region", { name: "Log" });
  await readOnly(log);
  await log.getByRole("button", { name: "Strategy phase · Round 1" }).click();
  await expect(pane("Map")).toHaveAttribute("aria-pressed", "true");
  const turn = page.getByRole("region", { name: "Turn" });
  await expect(turn.getByRole("heading", { name: "Strategy phase" })).toBeVisible();
  await expect(turn.locator("ul[aria-label=Picks] > li")).toHaveCount(8);
  const width = page.viewportSize()!.width;
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
  expect(await turn.evaluate((element) => element.scrollWidth - element.clientWidth)).toBe(0);
  await page.screenshot({ path: `${SHOTS}/phase.png`, animations: "disabled" });
  await page.getByRole("button", { name: "Back to now" }).click();
  await expect(turn).toHaveCount(0);
  expect(errors).toEqual([]);
});
