import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test, type Locator, type Page } from "@playwright/test";

// The shell of a game on desktop. Needs the engine: `scripts/build-wasm.sh`.
const WASM = fileURLToPath(new URL("../src/session/ti4.wasm", import.meta.url));
test.skip(!existsSync(WASM), "the engine is not built: run web2/scripts/build-wasm.sh");
test.use({ viewport: { width: 1920, height: 1080 } });

const SHOTS = process.env.WEB2_SHOTS ?? "shots/desktop/shell";

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

test("desktop: the right column is the players and the log, and the open step is a sheet over the board at the width of a phone", async ({
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
  const send = sheet.locator('footer button[aria-keyshortcuts="Enter"]');
  const heading = sheet.getByRole("heading").first();
  // The step of a tactical action is its selected tab: no heading says it again.
  const step = sheet.getByRole("tablist", { name: "Tactical action steps" }).getByRole("tab", {
    selected: true,
  });
  const row = (name: RegExp) => sheet.locator("#step-panel").getByRole("button", { name });
  const fits = async () =>
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(1920);

  // There are no panes: the column has the player table with eight seats, and the log under it.
  await expect(page.getByRole("navigation", { name: "Panes" })).toHaveCount(0);
  const players = page.getByRole("region", { name: "Players" });
  await expect(players.getByRole("listitem")).toHaveCount(8);
  const log = page.getByRole("region", { name: "Log" });
  // Before the first turn of the viewer the Log is the round in play, where nobody played yet.
  await expect(log).toContainText("No turns in this round yet");
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
  // The picker is the viewer's decision: the Log has no copy of it, and no second "?".
  await expect(log.getByRole("article", { name: "Current action" })).toHaveCount(0);
  await expect(log).not.toContainText("Activate a system");
  await expect(log.getByRole("button", { name: /^Rules/ })).toHaveCount(0);

  // The shortcut keys of the open step work as before: they read the step, not the screen.
  await page.keyboard.press("t");
  await expect(step).toContainText("Activation");
  // The sheet has the row of the context and its controls first, then the step tabs, then the
  // step; the heading row is the context of the action, not the name of the step.
  await expect(heading).toHaveText(/^Tactical action · /);
  await expect(sheet.getByRole("heading", { name: "Activation", exact: true })).toHaveCount(0);
  const headingBox = (await heading.boundingBox())!;
  const tabsBox = (await sheet
    .getByRole("tablist", { name: "Tactical action steps" })
    .boundingBox())!;
  const panelBox = (await sheet.locator("#step-panel").boundingBox())!;
  expect(headingBox.y).toBeLessThan(tabsBox.y);
  expect(tabsBox.y).toBeLessThan(panelBox.y);
  await expect(page.locator("g.system.target:not(.plain)")).not.toHaveCount(0);
  await page.locator("g.system.target:not(.plain)").first().click();
  await expect(send).toBeEnabled();
  await send.click();
  await expect(step).toContainText("Movement");
  await fits();
  await page.screenshot({ path: `${SHOTS}/movement.png`, animations: "disabled" });
  // The movement of the draft is the viewer's decision, and private: the Log has no copy of it,
  // and no turn of the viewer that is played now.
  await expect(log.getByRole("article", { name: "Current action" })).toHaveCount(0);
  await expect(log.getByRole("list", { name: "Turns" })).not.toContainText("▶");
  await expect(log).not.toContainText("Movement");
  await readOnly(log);
  // An open fleet has the room of the step sheet, and its check mark gives it back.
  await page.locator("g.system").filter({ hasText: "#01" }).click();
  const fleet = page.getByRole("group", { name: /^Move from / });
  await expect(fleet).toBeVisible();
  await expect(sheet).toHaveCount(0);
  await page.screenshot({ path: `${SHOTS}/fleet.png`, animations: "disabled" });
  await fleet.getByRole("button", { name: "Done with this system" }).click();
  await expect(step).toContainText("Movement");
  expect(errors).toEqual([]);
});

test("desktop: with no decision of the viewer open there is no step sheet, the footer keeps its buttons, and the Log has the phase that is played", async ({
  page,
}) => {
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
  // Whom the game waits for is the status of the toolbar; the board is free.
  await expect(page.locator("header strong")).toHaveText("Waiting for Hacan");
  await expect(sheet).toHaveCount(0);
  await expect(page.locator("#step-panel")).toHaveCount(0);
  // The footer alone stays over the board while it has a button: here the undo and the step.
  await expect(page.locator("footer").getByRole("button", { name: "Undo" })).toBeVisible();
  // The Log has the phase that is played now as its first row, with "▶".
  const log = page.getByRole("region", { name: "Log" });
  const live = log.getByRole("list", { name: "Turns" }).locator(":scope > li").first();
  await expect(live).toContainText("▶");
  await expect(live).toContainText("Strategy phase · Round 1");
  await readOnly(log);
  await page.screenshot({ path: `${SHOTS}/waiting.png`, animations: "disabled" });
  // A line over the board says what is played now; it opens Watch, the one sheet over the board,
  // with no bar of a decision and no footer.
  const board = page.getByRole("region", { name: "Galaxy board" });
  const line = board.getByRole("button", { name: /^Played now: .*Waiting for Hacan/ });
  await expect(line).toBeVisible();
  await line.click();
  await oneSheet(page, "Watch");
  await expect(line).toHaveCount(0);
  await expect(page.getByRole("region", { name: "Your decision" })).toHaveCount(0);
  await expect(page.locator("footer").getByRole("button", { name: "Undo" })).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(line).toBeVisible();

  expect(errors).toEqual([]);
});

test("desktop: while the other seats follow a strategy card of the viewer, the Log has its turn as the live row", async ({
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
  await expect(log.getByRole("article", { name: "Current action" })).toHaveCount(0);
  await readOnly(log);
  await page.screenshot({ path: `${SHOTS}/strategic.png`, animations: "disabled" });
  expect(errors).toEqual([]);
});

test("desktop: while the viewer decides a secondary, the sheet has its row and its choice, and the Log the turn of Sol", async ({
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
  const send = sheet.locator('footer button[aria-keyshortcuts="Enter"]');
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

  // The sheet: the row of the viewer with its choice, and no row of another seat.
  await expect(sheet).toContainText("Your decision");
  await expect(sheet.getByRole("button", { name: "Add bought token" })).toBeVisible();
  await expect(sheet).not.toContainText("Resolved");
  await expect(sheet).not.toContainText("Then: ");
  await expect(sheet.getByRole("button", { name: /^Rules/ })).toHaveCount(1);

  // The Log: the turn of Sol is its first row, live, with no copy of the choice.
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
  await page.screenshot({ path: `${SHOTS}/secondary.png`, animations: "disabled" });
  expect(errors).toEqual([]);
});

test("desktop: the step sheet minimizes to its heading line, its main button still sends, and a new question opens it again", async ({
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
  const send = sheet.locator('footer button[aria-keyshortcuts="Enter"]');
  const heading = sheet.getByRole("heading").first();
  const step = sheet.getByRole("tablist", { name: "Tactical action steps" }).getByRole("tab", {
    selected: true,
  });
  const row = (name: RegExp) => sheet.locator("#step-panel").getByRole("button", { name });
  const minimize = sheet.getByRole("button", { name: "Minimize" });
  const restore = sheet.getByRole("button", { name: "Restore" });

  // A staged card stays staged while the sheet is minimized: one line and the footer.
  await row(/^8\. Imperial$/).click();
  const title = (await heading.textContent())!;
  await minimize.click();
  await expect(sheet.locator("#step-panel")).toHaveCount(0);
  await expect(restore).toContainText(title);
  const boardBox = (await board.boundingBox())!;
  const small = (await sheet.boundingBox())!;
  expect(small.height).toBeLessThan(140);
  expect(small.height).toBeLessThan(boardBox.height * 0.15);
  await page.screenshot({ path: `${SHOTS}/minimized.png`, animations: "disabled" });
  // The line restores it, with what was staged.
  await restore.click();
  await expect(row(/^8\. Imperial$/)).toHaveAttribute("aria-pressed", "true");
  await minimize.click();
  // The main button sends from the minimized sheet; the next question opens the sheet again.
  await expect(send).toBeEnabled();
  await send.click();
  await expect(row(/^Activate a system$/)).toBeVisible();

  // A choice on the board while minimized keeps it minimized; the next step opens it.
  await page.keyboard.press("t");
  await expect(step).toContainText("Activation");
  await minimize.click();
  await expect(restore).toContainText("Activation");
  await page.locator("g.system.target:not(.plain)").first().click();
  await expect(restore).toBeVisible();
  await expect(send).toBeEnabled();
  await page.screenshot({ path: `${SHOTS}/minimized-activation.png`, animations: "disabled" });
  await send.click();
  await expect(step).toContainText("Movement");
  await expect(minimize).toBeVisible();
  expect(errors).toEqual([]);
});

test("desktop: the Log has a large row for each turn of the other seats, with its outcome", async ({
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
  const sheet = page.getByRole("region", { name: "Open step" });
  const row = (name: RegExp) => sheet.locator("#step-panel").getByRole("button", { name });
  // The viewer has the last card: the seven other seats play their first turn before it.
  await row(/^8\. Imperial$/).click();
  await sheet.locator('footer button[aria-keyshortcuts="Enter"]').click();
  await expect(row(/^Activate a system$/)).toBeVisible();

  const log = page.getByRole("region", { name: "Log" });
  const turns = log.getByRole("list", { name: "Turns" });
  // The strategy phase is a row; the start of the action phase is none.
  await expect(turns).toContainText("Strategy phase · Round 1");
  await expect(turns).not.toContainText("Action phase");
  // One row for each turn: the seat, the action and its outcome, with no dice.
  const rows = turns.locator("li > article, li > button[aria-label^='Turn ']");
  await expect(rows).toHaveCount(7);
  for (const turn of await rows.all()) {
    await expect(turn).toHaveAttribute(
      "aria-label",
      /^Turn \d+ · Seat [B-H] · (Tactical|Strategic|Component|Pass|Transaction)/,
    );
    await expect(turn).toContainText("Round 1 · turn");
    // A row is large: the seat, the action and the outcome are lines of their own.
    expect((await turn.boundingBox())!.height).toBeGreaterThan(40);
  }
  await expect(rows.filter({ hasText: "Strategic · Trade" })).toContainText("followed");
  await expect(log.locator("[aria-label=Rolls], .die")).toHaveCount(0);
  // Every id is named as the page names it, and nothing is cut at the side.
  const text = await log.innerText();
  expect(text).not.toMatch(/\b[a-z]+_[a-z0-9_]+\b/);
  expect(text).not.toMatch(/\bSeat [a-h]\b/);
  expect(await log.evaluate((element) => element.scrollWidth - element.clientWidth)).toBe(0);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(1920);
  await readOnly(log);
  await page.screenshot({ path: `${SHOTS}/log.png`, animations: "disabled" });
  expect(errors).toEqual([]);
});

test("desktop: a tactical turn of the Log opens in the place of the step sheet, read-only, in the history mode, with its steps, the dice of its combat and its system marked", async ({
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
  // Sol's space combat at #29 is under way, and Sol is the viewer.
  await page.goto("/?scenario=ongoing_combat&humans=1");
  const sheet = page.getByRole("region", { name: "Open step" });
  const send = sheet.locator('footer button[aria-keyshortcuts="Enter"]');
  const log = page.getByRole("region", { name: "Log" });
  const row = log.getByRole("button", { name: /· Tactical/ });
  // The viewer answers until the first round of the combat is rolled and its hits are taken.
  for (let answer = 0; answer < 12 && !/destroyed 3/.test(await row.innerText()); answer += 1) {
    if (await send.isEnabled()) {
      await send.click();
    } else {
      await sheet.locator("#step-panel button:not([aria-label=Minimize])").first().click();
    }
  }
  await expect(row).toContainText("destroyed 3");

  // The row opens the turn in the place of the step sheet; the decision is its line.
  await row.click();
  await expect(row).toHaveAttribute("aria-pressed", "true");
  await expect(row).toContainText("Open");
  const turn = page.getByRole("region", { name: "Turn" });
  await expect(turn).toBeVisible();
  expect((await turn.boundingBox())!.width).toBe(412);
  const tabs = turn.getByRole("tablist", { name: "Tactical action steps" }).getByRole("tab");
  await expect(tabs).toHaveText([/Activation/, /Movement/, /Space combat/]);
  await expect(tabs.last()).toHaveAttribute("aria-selected", "true");
  // The selected tab names the step; the heading row is the context of the action, above the tabs.
  await expect(turn.getByRole("heading", { name: "Space combat" })).toHaveCount(0);
  await expect(turn.getByRole("heading").first()).toHaveText(/^Tactical action · /);
  expect((await turn.getByRole("heading").first().boundingBox())!.y).toBeLessThan(
    (await tabs.first().boundingBox())!.y,
  );
  await expect(turn).toContainText("Played now");
  // Each roll of the combat is a table, with the dice on its rows.
  const rolls = turn.getByRole("group", { name: "Rolls" });
  await expect(rolls.getByRole("button")).toHaveText(["Anti-fighter barrage", "Round 1"]);
  await expect(turn.getByRole("img", { name: /^Rolled \d+: (hit|miss)$/ }).first()).toBeVisible();
  await rolls.getByRole("button", { name: "Anti-fighter barrage" }).click();
  await expect(turn.getByRole("img", { name: /^Rolled \d+: (hit|miss)$/ })).toHaveCount(4);
  // Nothing of the turn is sent: no field, and no button in its footer.
  await expect(turn.locator("input, select, textarea, footer button")).toHaveCount(0);
  // Its system is marked on the board.
  const board = page.getByRole("region", { name: "Galaxy board" });
  await expect(
    board.locator('[aria-label*="the open window or turn is about this system"]'),
  ).toHaveAttribute("aria-label", /system 29/);
  // The screen is in the history mode: the frame has its colour, the toolbar says which point of
  // the game the screen shows, and the stamp on the board which step.
  const main = page.locator("main");
  expect(await main.evaluate((element) => element.style.getPropertyValue("--accent"))).toBe(
    "var(--color-muted)",
  );
  const header = page.locator("header");
  await expect(header).toContainText("History");
  await expect(header).toContainText(/Round 1 · Turn \d+ · /);
  await expect(header.getByRole("navigation", { name: "Workspace" })).toHaveCount(0);
  await expect(header.getByRole("button", { name: "Back to now" })).toBeVisible();
  await expect(board.getByRole("status", { name: "Board shows" })).toHaveText(
    /^Round 1 · Turn \d+ · after Space combat$/,
  );
  // The toolbar has "‹" and "›" to the rows of the Log around it; the live row is the last. The
  // sheet has no line of its own for them.
  const turns = header.getByRole("navigation", { name: "Turns of the Log" });
  await expect(turns).toContainText(/Round 1 · Turn \d+ · /);
  await expect(turns.getByRole("button", { name: "Newer turn" })).toBeDisabled();
  await expect(turn.getByRole("navigation", { name: "Turns of the Log" })).toHaveCount(0);
  await expect(turn.getByRole("button", { name: "Close" })).toHaveCount(0);
  // No control of the live game: the decision is a bar that says that it waits, with no footer.
  await expect(sheet).toHaveCount(0);
  await expect(page.locator('footer button[aria-keyshortcuts="Enter"]')).toHaveCount(0);
  await expect(page.getByText("This commits to the live game")).toHaveCount(0);
  const waiting = page.getByRole("region", { name: "Your decision" });
  await expect(waiting).toContainText("Your decision is waiting: ");
  // It was open when the mode began: it is not new.
  await expect(waiting.locator("[data-mark]")).toHaveCount(0);
  // The keys of the live action do nothing.
  await page.keyboard.press("Enter");
  await expect(turn).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(1920);
  await page.screenshot({ path: `${SHOTS}/turn.png`, animations: "disabled" });

  // The movement: what left each system, and the fleet in #29 now → after.
  await tabs.nth(1).click();
  await expect(tabs.nth(1)).toHaveAttribute("aria-selected", "true");
  await expect(board.getByRole("status", { name: "Board shows" })).toHaveText(/after Movement$/);
  await expect(turn).toContainText("4 ships · 3 cargo");
  await expect(turn).toContainText("0 → 2");
  await expect(turn).not.toContainText("Choose a system on the map");
  await page.screenshot({ path: `${SHOTS}/turn-movement.png`, animations: "disabled" });

  // "▾" minimizes the turn to one line, and the screen stays in the history mode; the line gives
  // it back.
  await turn.getByRole("button", { name: "Minimize" }).click();
  await expect(tabs).toHaveCount(0);
  await expect(turn.getByRole("button", { name: "Restore" })).toContainText("Movement");
  await expect(header).toContainText("History");
  await turn.getByRole("button", { name: "Restore" }).click();
  await expect(tabs.nth(1)).toHaveAttribute("aria-selected", "true");

  // Esc closes the turn, and the decision is back.
  await page.keyboard.press("Escape");
  await expect(turn).toHaveCount(0);
  await expect(row).toHaveAttribute("aria-pressed", "false");
  await expect(sheet.locator("#step-panel")).toBeVisible();
  expect(await main.evaluate((element) => element.style.getPropertyValue("--accent"))).not.toBe(
    "var(--color-muted)",
  );
  await expect(header.getByRole("navigation", { name: "Workspace" })).toBeVisible();
  // So does "Back" of the bar of the decision, and "Back to now" of the toolbar.
  await row.click();
  await expect(turn).toBeVisible();
  await waiting.getByRole("button", { name: "Back" }).click();
  await expect(turn).toHaveCount(0);
  await expect(sheet.locator("#step-panel")).toBeVisible();
  await row.click();
  await header.getByRole("button", { name: "Back to now" }).click();
  await expect(turn).toHaveCount(0);
  await expect(sheet.locator("#step-panel")).toBeVisible();
  expect(errors).toEqual([]);
});

test("desktop: a strategic turn of the Log opens to its card and a row for each seat, each with what it did, line by line one click further", async ({
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
  // Sol and Hacan are played here: Sol plays Leadership, Hacan follows it and buys a token.
  await page.goto("/?scenario=strategy_leadership&humans=3");
  const sheet = page.getByRole("region", { name: "Open step" });
  const send = sheet.locator('footer button[aria-keyshortcuts="Enter"]');
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
  await sheet.getByRole("button", { name: "Add bought token" }).click();
  await sheet.getByRole("button", { name: "Add fleet pool token" }).click();
  await page.getByRole("button", { name: "Auto-pay" }).click();
  await send.click();

  const log = page.getByRole("region", { name: "Log" });
  const row = log.getByRole("button", { name: /· Strategic · Leadership/ });
  await expect(row).toContainText("1 followed");
  await row.click();
  await expect(row).toHaveAttribute("aria-pressed", "true");
  const turn = page.getByRole("region", { name: "Turn" });
  await expect(turn).toBeVisible();
  expect((await turn.boundingBox())!.width).toBe(412);
  await expect(turn.getByRole("heading", { name: "Leadership" })).toBeVisible();
  await expect(turn).toContainText("Strategic action");
  // The primary, then a row for each other seat with its answer and what it did in short.
  const rows = turn.locator("ul[aria-label=Seats] > li");
  await expect(rows.first()).toContainText("✓ Resolved");
  await expect(rows.first()).toContainText("gained 3 command tokens");
  const hacan = rows.filter({ hasText: "Hacan" });
  await expect(hacan).toContainText("✓ Followed");
  await expect(hacan).toContainText("gained 1 command token");
  await expect(rows.filter({ hasText: "– Passed" })).not.toHaveCount(0);
  // The row of Hacan opens to what it did line by line: the influence it spent and the token.
  const open = hacan.getByRole("button", { expanded: false });
  await open.click();
  await expect(hacan.getByRole("button", { expanded: true })).toBeVisible();
  await expect(hacan.getByRole("list", { name: "What it did" })).toContainText("Spent influence");
  await expect(hacan.getByRole("list", { name: "What it did" })).toContainText(
    "Gained 1 fleet token",
  );
  // Nothing of the turn is sent: no field, and no button in its footer.
  await expect(turn.locator("input, select, textarea, footer button")).toHaveCount(0);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(1920);
  await readOnly(log);
  await page.screenshot({ path: `${SHOTS}/strategic-turn.png`, animations: "disabled" });
  // "Back to now" closes it.
  await page.getByRole("button", { name: "Back to now" }).click();
  await expect(turn).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("desktop: a strategic turn with a tactical action opens to its card, then the steps of its tactical action as a tactical turn has them", async ({
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
  const log = page.getByRole("region", { name: "Log" });
  const row = log.getByRole("button", { name: /· Strategic · Warfare/ });
  // Sol's turn is played now: its row opens Watch, in the present.
  await row.click();
  const turn = page.getByRole("region", { name: "Watch" });
  const tabs = turn.getByRole("tab");
  await expect(tabs.first()).toHaveText(/Warfare/);
  await expect(tabs.nth(1)).toHaveText(/Activation/);
  await expect(tabs.first()).toHaveAttribute("aria-selected", "true");
  // The tab names the card: no heading says it again.
  await expect(turn.getByRole("heading", { name: "Warfare" })).toHaveCount(0);
  await expect(turn).toContainText("Strategic action · ");
  // The card's seats; the primary does not say the steps of its tactical action again.
  const primary = turn.locator("ul[aria-label=Seats] > li").first();
  await expect(primary).toContainText("P");
  await expect(primary).not.toContainText("Activated");
  await expect(primary).not.toContainText("Round");
  // The activation is a step of its own; Watch keeps the live board, with no stamp.
  await tabs.nth(1).click();
  await expect(tabs.nth(1)).toHaveAttribute("aria-selected", "true");
  await expect(
    page.getByRole("region", { name: "Galaxy board" }).getByRole("status", { name: "Board shows" }),
  ).toHaveCount(0);
  await expect(turn.locator("input, select, textarea, footer button")).toHaveCount(0);
  await page.screenshot({ path: `${SHOTS}/warfare-turn.png`, animations: "disabled" });
  expect(errors).toEqual([]);
});

test("desktop: a phase of the Log opens what the seats did in it, and the live row of another seat opens Watch in the place of the decision", async ({
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
  await page.goto("/?local=3&players=8&humans=1");
  const sheet = page.getByRole("region", { name: "Open step" });
  const pick = (name: RegExp) => sheet.locator("#step-panel").getByRole("button", { name });
  await pick(/^8\. Imperial$/).click();
  await sheet.locator('footer button[aria-keyshortcuts="Enter"]').click();
  await expect(pick(/^Activate a system$/)).toBeVisible();

  // The strategy phase opens to the picks in their order, read-only, in the place of the sheet.
  const log = page.getByRole("region", { name: "Log" });
  await readOnly(log);
  const phase = log.getByRole("button", { name: "Strategy phase · Round 1" });
  await phase.click();
  await expect(phase).toHaveAttribute("aria-pressed", "true");
  const turn = page.getByRole("region", { name: "Turn" });
  expect((await turn.boundingBox())!.width).toBe(412);
  await expect(turn.getByRole("heading", { name: "Strategy phase" })).toBeVisible();
  // The history mode says that it is the past: no pill "Resolved" in the sheet.
  await expect(page.locator("header")).toContainText("Round 1 · Strategy phase");
  await expect(
    page.getByRole("region", { name: "Galaxy board" }).getByRole("status", { name: "Board shows" }),
  ).toHaveText("Round 1 · end of the Strategy phase");
  await expect(turn.locator("aside > div").first()).not.toContainText("Resolved");
  const picks = turn.locator("ul[aria-label=Picks] > li");
  await expect(picks).toHaveCount(8);
  await expect(picks.filter({ hasText: "You" })).toContainText("Imperial");
  await expect(turn.locator("input, select, textarea, footer button")).toHaveCount(0);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(1920);
  await page.screenshot({ path: `${SHOTS}/phase.png`, animations: "disabled" });
  // "‹" and "›" go to the rows of the Log around it: the phase is the first row.
  const turns = page.locator("header").getByRole("navigation", { name: "Turns of the Log" });
  await expect(turns.locator("strong")).toHaveText("Round 1 · Strategy phase");
  await expect(turns.getByRole("button", { name: "Older turn" })).toBeDisabled();
  await turns.getByRole("button", { name: "Newer turn" }).click();
  await expect(phase).toHaveAttribute("aria-pressed", "false");
  await expect(log.getByRole("button", { pressed: true })).toHaveCount(1);
  await expect(turns.locator("strong")).toHaveText(/^Round 1 · Turn 1 · /);
  await expect(page.locator("header")).toContainText(/Round 1 · Turn 1 · /);
  await turns.getByRole("button", { name: "Older turn" }).click();
  await expect(phase).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("Escape");
  await expect(turn).toHaveCount(0);

  // Sol and Hacan are played here: Sol plays Leadership, then Hacan is asked its secondary.
  await page.goto("/?scenario=strategy_leadership&humans=3");
  await sheet
    .locator("#step-panel")
    .getByRole("button", { name: /Leadership/ })
    .first()
    .click();
  await sheet.locator('footer button[aria-keyshortcuts="Enter"]').click();
  for (let at = 0; at < 3; at++) {
    await sheet.getByRole("button", { name: "Add tactic pool token" }).click();
  }
  await sheet.locator('footer button[aria-keyshortcuts="Enter"]').click();
  await expect(sheet.getByRole("button", { name: "Add bought token" })).toBeVisible();
  // The action waits for the viewer: its decision is the one place, no line says it.
  const board = page.getByRole("region", { name: "Galaxy board" });
  const line = board.getByRole("button", { name: /^Played now: / });
  await expect(line).toHaveCount(0);
  await oneSheet(page, "Open step");
  // The live row is the turn of Sol: it opens Watch, what is played now, who decides and who
  // waits, in the present, in the place of the decision.
  const live = log.getByRole("button", { name: /· Strategic · Leadership/ });
  await expect(live).toContainText("▶");
  await live.click();
  const watch = page.getByRole("region", { name: "Watch" });
  await expect(watch).toContainText("Played now");
  expect((await watch.boundingBox())!.width).toBeLessThanOrEqual(412);
  const you = watch.locator("ul[aria-label=Seats] > li").filter({ hasText: "You" });
  await expect(you).toContainText("▶ Deciding");
  await oneSheet(page, "Watch");
  await expect(line).toHaveCount(0);
  // No history mode: the toolbar keeps the status and Live, the board is not hatched, no stamp.
  const header = page.locator("header");
  await expect(header).not.toContainText("History");
  await expect(header.getByRole("navigation", { name: "Workspace" })).toBeVisible();
  await expect(board.locator("[data-past]")).toHaveCount(0);
  await expect(board.getByRole("status", { name: "Board shows" })).toHaveCount(0);
  // The viewer's decision is its bar: no control of it, no footer, no key.
  const waiting = page.getByRole("region", { name: "Your decision" });
  await expect(waiting).toContainText("Your decision is waiting: ");
  await expect(page.locator('footer button[aria-keyshortcuts="Enter"]')).toHaveCount(0);
  await page.screenshot({ path: `${SHOTS}/live-turn.png`, animations: "disabled" });
  // "Back" gives the decision back; Esc closes Watch too.
  await waiting.getByRole("button", { name: "Back" }).click();
  await expect(watch).toHaveCount(0);
  await expect(sheet.getByRole("button", { name: "Add bought token" })).toBeVisible();
  await live.click();
  await expect(watch).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(watch).toHaveCount(0);
  await oneSheet(page, "Open step");
  expect(errors).toEqual([]);
});
