import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { activateInLive } from "./actions";

// A game on a real server (`crates/ti4-server`, started by playwright.config.ts), through the
// proxy of vite: the page has the first seat of eight, the other seats decide at random. The
// printed cards are those of the engine of the page: `scripts/build-wasm.sh`.
const WASM = fileURLToPath(new URL("../src/session/ti4.wasm", import.meta.url));
test.skip(!existsSync(WASM), "the engine is not built: run web2/scripts/build-wasm.sh");
test.use({ viewport: { width: 1920, height: 1080 } });

const panel = (page: Page) => page.locator("#step-panel");
const send = (page: Page) => page.locator('footer button[aria-keyshortcuts="Enter"]');
const heading = (page: Page) => panel(page).getByRole("heading").first();
const row = (page: Page, name: string | RegExp) => panel(page).getByRole("button", { name });
const system = (page: Page, id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
const workspace = (page: Page, name: string) =>
  page.getByRole("navigation", { name: "Workspace" }).getByRole("button", { name });
const status = (page: Page) => page.locator("header strong").first();
/** The header of a game that random bots play from its start. The speaker is drawn: a seat that
 * chooses last is given the last strategy card, and is asked in the action phase first, after the
 * seats before it took their turns. */
const round1 = /^Round 1 · (Strategy phase|Action phase( · Turn \d+)?)$/;

/** The errors of the page. A test expects none. */
function errorsOf(page: Page) {
  const errors: string[] = [];
  page.on("console", (message) => message.type() === "error" && errors.push(message.text()));
  page.on("pageerror", (error) => errors.push(error.message));
  return errors;
}

test("a seeded game on the server: the first turn, with a draft, the sheets and a reload", async ({
  page,
  request,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = errorsOf(page);
  // The game is made as the page makes it, with a seed, so that the board is the same each run.
  const response = await request.post("/api/games", {
    data: { player_count: 8, seed: 3, nickname: "Ada", fill_seats: "random" },
  });
  expect(response.ok()).toBe(true);
  const made = await response.json();
  await page.addInitScript(
    ([key, value]) => localStorage.setItem(key, value),
    [`ti4.online.${made.game_id}.session`, made.player_session],
  );
  await page.goto(`/?online=${made.game_id}`);
  await expect(page.getByText("Round 1 · Strategy phase")).toBeVisible();
  await expect(page.getByRole("region", { name: "Players" }).locator("tbody tr")).toHaveCount(8);
  await expect(heading(page)).toHaveText("Choose a strategy card");
  // The tools of a local game are not there, and the header leads to the start screen. The seat has
  // no decision of its own to take back yet.
  await expect(page.getByRole("button", { name: "Dev", exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Undo" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Start", exact: true })).toBeVisible();
  // The page kept one player session for the game before a browser could have several: it is
  // moved to the player once, and the address says which player the tab is.
  await expect(page).toHaveURL(new RegExp(`\\?online=${made.game_id}&seat=${made.player.id}$`));
  expect(
    await page.evaluate(
      (id) => [
        localStorage.getItem(`ti4.online.${id}.session`),
        localStorage.getItem(`ti4.online.${id}.players`),
      ],
      made.game_id,
    ),
  ).toEqual([null, JSON.stringify({ [made.player.id]: made.player_session })]);

  // The seat answers what it is asked, up to the menu of its first turn: the other seats play.
  for (let count = 0; ; count++) {
    expect(count).toBeLessThan(20);
    await expect(status(page)).toHaveText("Your decision");
    if (await row(page, /^Activate a system$/).count()) {
      break;
    }
    await panel(page).getByRole("button", { pressed: false }).first().click();
    await send(page).click();
    await expect(status(page)).not.toHaveText("Your decision");
  }
  await expect(page.getByText("Round 1 · Action phase")).toBeVisible();

  // The draft of the turn is played by the server, on a copy: each change is one request.
  await workspace(page, "Draft").click();
  await expect(heading(page)).toHaveText("Activation");
  await expect(page.getByText("Private draft").first()).toBeVisible();
  // The server draws the faction of the seat, so the test reads the board: the first plain system
  // that the ships of the seat reach (no ships there, no anomaly) is activated, and two ships of
  // the home system move there.
  const labels = await page
    .locator("g.system.target:not(.plain)")
    .evaluateAll((all) => all.map((one) => one.getAttribute("aria-label") ?? ""));
  const target = labels
    .map((label) => /^Activate [^,]+, system (\w+)$/.exec(label)?.[1])
    .find(Boolean)!;
  await system(page, target).click();
  await page.keyboard.press("Enter");
  await expect(heading(page)).toHaveText("Movement");
  // The row of the panel is there when the server has played the draft up to the movement.
  await panel(page)
    .getByRole("button", { name: /Nothing staged$/ })
    .first()
    .click();
  const sheet = page.getByRole("group", { name: /^Move from / });
  const ships = sheet.getByRole("button", {
    name: /^(?!Load )[A-Z][A-Za-z ]+ \d of \d$/,
    pressed: false,
  });
  await ships.first().click();
  await expect(sheet.getByText(/· 1 ship\b/)).toBeVisible();
  await ships.first().click();
  await expect(sheet.getByText(/· 2 ships\b/)).toBeVisible();
  await expect(system(page, target)).toHaveAttribute("aria-label", /, 2 [^,]+ ships$/);
  await expect(page.locator("footer").getByText("The action ends here.")).toBeVisible();
  await page.screenshot({ path: "shots/online/draft-movement.png", animations: "disabled" });

  // A reload resumes the game as the same player, and the draft is kept for this game.
  await page.reload();
  await expect(row(page, /^Open your draft$/)).toBeVisible();
  await expect(system(page, target)).not.toHaveAttribute("aria-label", / ships?$/);
  await workspace(page, "Draft").click();
  await expect(heading(page)).toHaveText("Movement");
  await expect(system(page, target)).toHaveAttribute("aria-label", /, 2 [^,]+ ships$/);

  // "Apply to Live" sends the action, the system and the movement as one request.
  const apply = page.getByRole("button", { name: "Apply to Live" });
  await expect(apply).toBeEnabled();
  await apply.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText(new RegExp(`^Activate .+ · #${target}$`))).toBeVisible();
  await dialog.getByRole("button", { name: "Apply to Live" }).click();
  await expect(workspace(page, "Live")).toHaveAttribute("aria-pressed", "true");
  await expect(system(page, target)).toHaveAttribute("aria-label", /, 2 [^,]+ ships$/);
  await expect(heading(page)).not.toHaveText("Movement");
  await page.screenshot({ path: "shots/online/after-draft.png", animations: "disabled" });

  // The seat may take its movement back: nothing was rolled or shown since, and no other seat
  // decided. One undo takes it back whole, and it is staged again as it was sent.
  await page.getByRole("button", { name: "Undo" }).click();
  await expect(heading(page)).toHaveText("Movement");
  await expect(status(page)).toHaveText("Your decision");
  await expect(system(page, target)).toHaveAttribute("aria-label", /, 2 [^,]+ ships$/);
  await page.screenshot({ path: "shots/online/after-undo.png", animations: "disabled" });
  await expect(send(page)).toHaveText(/Move fleet/);
  await send(page).click();
  await expect(heading(page)).not.toHaveText("Movement");
  await expect(system(page, target)).toHaveAttribute("aria-label", /, 2 [^,]+ ships$/);

  // The sheets have the facts that the server sends, as in a local game.
  await page.getByRole("button", { name: "Cards", exact: true }).click();
  const cards = page.getByRole("complementary", { name: "Cards" });
  await expect(cards.getByText(/^Hand \d+ \/ 7$/)).toBeVisible();
  await expect(cards.getByText(/of 122 cards left/)).toBeVisible();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Secrets", exact: true }).click();
  const secrets = page.getByRole("complementary", { name: "Secrets" });
  await expect(secrets.getByText(/^\d scored · \d \/ 3$/)).toBeVisible();
  await expect(secrets.getByText(/^\d+ of 40 unseen · \d+ in the deck$/)).toBeVisible();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Objectives", exact: true }).click();
  await expect(
    page.getByRole("complementary", { name: "Objectives" }).getByText("10 VP to win"),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Agendas", exact: true }).click();
  await expect(
    page.getByRole("complementary", { name: "Agendas" }).getByText(/in the deck$/),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  expect(errors).toEqual([]);
});

test("the same seat in two tabs: what one tab stages, the other shows", async ({
  page,
  context,
  request,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = errorsOf(page);
  const response = await request.post("/api/games", {
    data: { player_count: 8, seed: 3, nickname: "Ada", fill_seats: "random" },
  });
  expect(response.ok()).toBe(true);
  const made = await response.json();
  await context.addInitScript(
    ([key, value]) => localStorage.setItem(key, value),
    [
      `ti4.online.${made.game_id}.players`,
      JSON.stringify({ [made.player.id]: made.player_session }),
    ],
  );
  const address = `/?online=${made.game_id}&seat=${made.player.id}`;
  await page.goto(address);
  for (let count = 0; ; count++) {
    expect(count).toBeLessThan(20);
    await expect(status(page)).toHaveText("Your decision");
    if (await row(page, /^Activate a system$/).count()) {
      break;
    }
    await panel(page).getByRole("button", { pressed: false }).first().click();
    await send(page).click();
    await expect(status(page)).not.toHaveText("Your decision");
  }
  const other = await context.newPage();
  const otherErrors = errorsOf(other);
  await other.goto(address);
  await expect(row(other, /^Activate a system$/)).toBeVisible();

  // Tab A drafts the action; the draft of tab B follows. What is open is of each tab.
  await workspace(page, "Draft").click();
  await expect(heading(page)).toHaveText("Activation");
  const labels = await page
    .locator("g.system.target:not(.plain)")
    .evaluateAll((all) => all.map((one) => one.getAttribute("aria-label") ?? ""));
  const target = labels
    .map((label) => /^Activate [^,]+, system (\w+)$/.exec(label)?.[1])
    .find(Boolean)!;
  await system(page, target).click();
  await page.keyboard.press("Enter");
  await expect(heading(page)).toHaveText("Movement");
  await panel(page)
    .getByRole("button", { name: /Nothing staged$/ })
    .first()
    .click();
  const sheet = page.getByRole("group", { name: /^Move from / });
  const ships = sheet.getByRole("button", {
    name: /^(?!Load )[A-Z][A-Za-z ]+ \d of \d$/,
    pressed: false,
  });
  await ships.first().click();
  await ships.first().click();
  await expect(system(page, target)).toHaveAttribute("aria-label", /, 2 [^,]+ ships$/);
  await expect(workspace(other, "Live")).toHaveAttribute("aria-pressed", "true");
  await workspace(other, "Draft").click();
  await expect(heading(other)).toHaveText("Movement");
  await expect(system(other, target)).toHaveAttribute("aria-label", /, 2 [^,]+ ships$/);
  // One ship less in tab A is one ship less in tab B.
  await sheet
    .getByRole("button", { name: /^(?!Load )[A-Z][A-Za-z ]+ \d of \d$/, pressed: true })
    .first()
    .click();
  await expect(system(page, target)).toHaveAttribute("aria-label", /, 1 [^,]+ ship$/);
  await expect(system(other, target)).toHaveAttribute("aria-label", /, 1 [^,]+ ship$/);

  // Tab A plays the action live, and stages the movement: tab B shows the same tokens and marks.
  // The draft starts over, so that only the system is applied.
  await page.getByRole("button", { name: "More draft actions" }).click();
  await page.getByRole("menuitem", { name: "Start over" }).click();
  await workspace(page, "Live").click();
  await workspace(other, "Live").click();
  await activateInLive(page, system(page, target));
  await expect(heading(page)).toHaveText("Movement");
  await expect(heading(other)).toHaveText("Movement");
  await panel(page)
    .getByRole("button", { name: /Nothing staged$/ })
    .first()
    .click();
  await ships.first().click();
  await ships.first().click();
  await expect(system(page, target)).toHaveAttribute("aria-label", /, 2 [^,]+ ships$/);
  await expect(system(other, target)).toHaveAttribute("aria-label", /, 2 [^,]+ ships$/);
  await expect(page.locator('g.system[aria-label*="handled"]')).toHaveCount(1);
  await expect(other.locator('g.system[aria-label*="handled"]')).toHaveCount(1);
  await expect(send(other)).toHaveText(/Move fleet/);
  await other.screenshot({ path: "shots/online/same-seat-movement.png", animations: "disabled" });
  expect(errors).toEqual([]);
  expect(otherErrors).toEqual([]);
});

test("the quick start is a game on the server, at its address", async ({ page }) => {
  const errors = errorsOf(page);
  await page.goto("/?online&fill=random");
  await expect(page).toHaveURL(/\?online=[^&]+&seat=[^&]+$/);
  // The page asks the table first, then loads the engine and opens the game.
  await expect(page.getByText(round1)).toBeVisible();
  await expect(page.getByRole("region", { name: "Players" }).locator("tbody tr")).toHaveCount(8);
  // The page plays a seat: it is asked, or it waits for a seat before it.
  await expect(status(page)).toHaveText(/^(Your decision|Waiting for .+|Turn of .+)$/);
  // The player of this browser is kept for the game, and the address says which it is: a
  // reload is the same seat.
  const address = new URL(page.url()).searchParams;
  const players = await page.evaluate(
    (key) => JSON.parse(localStorage.getItem(key) ?? "{}"),
    `ti4.online.${address.get("online")}.players`,
  );
  expect(Object.keys(players)).toEqual([address.get("seat")]);
  await page.reload();
  await expect(page.getByText(round1)).toBeVisible();
  await expect(status(page)).toHaveText(/^(Your decision|Waiting for .+|Turn of .+)$/);
  expect(errors).toEqual([]);
});

test("the quick start with a seed, a name and a map asks the server for that game", async ({
  page,
}) => {
  const asked = page.waitForRequest(
    (request) => request.method() === "POST" && new URL(request.url()).pathname === "/api/games",
  );
  await page.goto("/?online&fill=random&seed=3&name=Ada&map=random");
  expect((await asked).postDataJSON()).toEqual({
    player_count: 8,
    nickname: "Ada",
    fill_seats: "random",
    seed: 3,
    map: "random",
  });
  // The address of the game has no part of the start: a reload is the game, not another one.
  await expect(page).toHaveURL(/\?online=[^&]+&seat=[^&]+$/);
  await expect(page.getByText("Round 1 · Strategy phase")).toBeVisible();
});

test("a game that the server no longer knows is said, with the way to the start screen", async ({
  page,
}) => {
  await page.goto("/?online=game_that_is_gone");
  // The page does not try for ever: the server says that it has no such game.
  await expect(page.getByRole("status")).toHaveText("The server no longer knows this game.");
  await page.screenshot({ path: "shots/online/unknown-game.png", animations: "disabled" });
  await page.getByRole("button", { name: "Start", exact: true }).click();
  await page.waitForURL((url) => url.pathname === "/" && !url.searchParams.has("online"));
  await expect(page.getByRole("link", { name: /Multiplayer/ })).toBeVisible();
});

test("a lost connection is said in the toolbar, and the game comes back by itself", async ({
  page,
}) => {
  const errors = errorsOf(page);
  // The sockets of the page go through the test, which can cut them and refuse new ones.
  let down = false;
  const open: { close(): Promise<void> }[] = [];
  await page.routeWebSocket(/\/ws\/games\//, (socket) => {
    if (down) {
      return void socket.close();
    }
    socket.connectToServer();
    open.push(socket);
  });
  await page.goto("/?online&fill=random");
  await expect(page.getByText(round1)).toBeVisible();
  // The bots play until the seat is asked; then the game waits for it, also while it is offline.
  await expect(status(page)).toHaveText("Your decision");

  down = true;
  await Promise.all(open.map((socket) => socket.close()));
  // One signal, and no error: the game stays in view, and nothing can be sent.
  await expect(status(page)).toHaveText("Offline · reconnecting");
  await expect(page.getByRole("region", { name: "Players" }).locator("tbody tr")).toHaveCount(8);
  await expect(send(page)).toHaveCount(0);
  // The note is in the panel of a waiting seat. The secondary of a strategic action keeps the
  // overview of the card, which has no note: the toolbar alone says it there.
  const strategic = page.getByRole("complementary").getByText("Strategic action", { exact: true });
  await expect(panel(page).getByText("The game cannot be reached.").or(strategic)).toBeVisible();
  await page.screenshot({ path: "shots/online/offline.png", animations: "disabled" });

  // The page tries again by itself, and the snapshot after it is the game as it is.
  down = false;
  await expect(status(page)).toHaveText("Your decision");
  expect(errors).toEqual([]);
});
