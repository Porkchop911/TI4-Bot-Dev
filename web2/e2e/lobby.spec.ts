import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import {
  expect,
  test,
  type APIRequestContext,
  type BrowserContext,
  type Page,
} from "@playwright/test";

// The start screen and the lobby of a table, against a real server (`crates/ti4-server`,
// started by playwright.config.ts) through the proxy of vite. A table is made, filled and
// started from the page, and several tabs of one browser are several seats of its game.
const WASM = fileURLToPath(new URL("../src/session/ti4.wasm", import.meta.url));
test.skip(!existsSync(WASM), "the engine is not built: run web2/scripts/build-wasm.sh");
test.use({ viewport: { width: 1920, height: 1080 } });

/** The two long names of the demo data: a name is cut, never the row. */
const LONG = [
  "Bartholomew Featherstonehaugh the Third of Mecatol",
  "Wilhelmina Vanderbilt-Montgomery",
];

const players = (page: Page) => page.getByRole("region", { name: "Players" });
/** The seats of the table. An open choice of a faction is a list inside its row. */
const rows = (page: Page) => players(page).locator("ul.overflow-y-auto > li");
/** The control of the faction in the row of a seat, and the choice that it opens. */
const factionOf = (page: Page, name: string) =>
  players(page).getByRole("button", { name: new RegExp(`^Faction of ${name}: `) });
const choice = (page: Page, name: string) =>
  players(page).getByRole("group", { name: `Faction of ${name}` });
const setup = (page: Page) => page.getByRole("region", { name: "Setup" });
/** The value of a setup row: the number of a counter, or the list. */
const value = (page: Page, label: string) => setup(page).getByLabel(label, { exact: true });
/** The value of a setup row as words, for a viewer with no control there. */
const said = (page: Page, label: string) =>
  setup(page).locator("dt", { hasText: label }).locator("xpath=following-sibling::dd");
const note = (page: Page) => page.locator("#lobby-note");
const main = (page: Page) => page.locator('button[aria-keyshortcuts="Enter"]');
const title = (page: Page) => page.locator("header strong").first();
const toast = (page: Page, text: string) => page.getByRole("status").filter({ hasText: text });
/** The action panel of the running game, and its main button. */
const panel = (page: Page) => page.locator("#step-panel");
const send = (page: Page) => page.locator('footer button[aria-keyshortcuts="Enter"]');
/** The seat of the page in the running game: the row of the player table that is marked as its own. */
const ownSeat = (page: Page) =>
  page.getByRole("region", { name: "Players" }).locator('tbody tr[class*="bg-cyan"] th button');
/** The player table of the running game: one row for each seat. */
const seatRows = (page: Page) => page.getByRole("region", { name: "Players" }).locator("tbody tr");
/** The sheet "Table" of the running game. */
const sheet = (page: Page) => page.getByRole("complementary", { name: "Table" });

/** The errors of the page. A test expects none. */
function errorsOf(page: Page) {
  const errors: string[] = [];
  page.on("console", (message) => message.type() === "error" && errors.push(message.text()));
  page.on("pageerror", (error) => errors.push(error.message));
  return errors;
}

/** A table that is made through the calls of the lobby, with its host. */
async function makeTable(request: APIRequestContext, count: number, nickname: string) {
  const response = await request.post("/api/games", {
    data: { player_count: count, seed: 11, nickname },
  });
  expect(response.ok()).toBe(true);
  const made = await response.json();
  return {
    id: made.game_id as string,
    host: { id: made.player.id as string, session: made.player_session as string },
    async call(path: string, data: object, session: string | null = made.player_session) {
      const answer = await request.post(`/api/games/${made.game_id}/lobby/${path}`, {
        data,
        headers: session ? { "x-ti4-player-session": session } : {},
      });
      expect(answer.ok(), `${path}: ${await answer.text()}`).toBe(true);
      return answer.json();
    },
  };
}

/** The page is the player `id` of the table: what the browser keeps of a player. */
const asPlayer = (page: Page, game: string, id: string, session: string) =>
  page.addInitScript(
    ([key, value]) => localStorage.setItem(key, value),
    [`ti4.online.${game}.players`, JSON.stringify({ [id]: session })],
  );

/** The players that this browser has at a table: each tab is the one of its address. */
const asPlayers = (context: BrowserContext, game: string, players: Record<string, string>) =>
  context.addInitScript(
    ([key, value]) => localStorage.setItem(key, value),
    [`ti4.online.${game}.players`, JSON.stringify(players)],
  );

test("three tabs of one browser make a table from the start screen, fill it and play its game", async ({
  page,
  context,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = errorsOf(page);
  // The start screen: the name, then "Multiplayer", which makes a table of four.
  await page.goto("/");
  await page.getByLabel("Name").fill("Ada");
  const asked = page.waitForRequest(
    (request) => request.method() === "POST" && new URL(request.url()).pathname === "/api/games",
  );
  await page.getByRole("link", { name: /Multiplayer/ }).click();
  expect((await asked).postDataJSON()).toEqual({ player_count: 4, nickname: "Ada" });
  await expect(page).toHaveURL(/\?online=[^&]+&seat=[^&]+$/);
  const game = new URL(page.url()).searchParams.get("online")!;

  // The lobby: the host, three open seats, and a start that waits for them.
  await expect(title(page)).toHaveText("Lobby · 4 players");
  await expect(rows(page)).toHaveCount(4);
  await expect(rows(page).nth(0)).toContainText("Ada (you)");
  await expect(rows(page).nth(0).getByRole("img", { name: "Host" })).toBeVisible();
  await expect(rows(page).nth(0)).toContainText("Random");
  await expect(rows(page).nth(1).getByRole("img", { name: "Open seat" })).toBeVisible();
  await expect(players(page)).toContainText("1 of 4");
  await expect(value(page, "Players")).toHaveText("4");
  await expect(value(page, "Strategy cards")).toHaveValue("te");
  await expect(note(page)).toHaveText("Waiting for 3 open seats");
  await expect(main(page)).toHaveText(/Start game/);
  await expect(main(page)).toBeDisabled();
  // The board is the map that the table will play, with the places of the four seats.
  await expect(page.locator("g.system")).not.toHaveCount(0);
  await expect(page.locator("g.system.home")).toHaveCount(4);
  // What the kinds of a seat do is behind "?", not on the screen.
  await expect(page.getByText("The server plays this seat by rules")).toHaveCount(0);
  await players(page).getByRole("button", { name: "About the kinds of a seat" }).click();
  await expect(page.getByText("The server plays this seat by rules")).toBeVisible();
  await page.keyboard.press("Escape");
  await page.screenshot({ path: "shots/online/lobby-host.png", animations: "disabled" });

  // A second and a third tab join as other players of this browser.
  const tabs: Page[] = [];
  for (const name of ["Ada 2", "Ada 3"]) {
    const opened = context.waitForEvent("page");
    await players(page).getByRole("button", { name: "Join in a new tab" }).click();
    const tab = await opened;
    await tab.waitForURL(/\?online=[^&]+&seat=[^&]+$/);
    await expect(rows(tab).filter({ hasText: "(you)" })).toContainText(name);
    // The first tab sees the player who joined, with no reload and no wait for a poll.
    await expect(rows(page).filter({ hasText: name })).toHaveCount(1, { timeout: 2_000 });
    tabs.push(tab);
  }
  const [second, third] = tabs;
  const seats = [page, second, third].map((tab) => new URL(tab.url()).searchParams.get("seat"));
  expect(new Set(seats).size).toBe(3);
  await expect(players(page)).toContainText("3 of 4");
  // A player who is not the host has the same screen, with no controls of the table.
  await expect(rows(second)).toHaveCount(4);
  await expect(main(second)).toHaveCount(0);
  await expect(note(second)).toHaveText("Waiting for 1 open seat");
  await expect(players(second).getByRole("button", { name: "Rule bot" })).toHaveCount(0);
  await expect(players(second).getByRole("button", { name: /^Remove / })).toHaveCount(0);
  // The other tabs of this browser are marked for the host: "(yours)".
  await expect(rows(page).filter({ hasText: "(yours)" })).toHaveCount(2);

  // A reload of a tab keeps its seat.
  await second.reload();
  await expect(rows(second).filter({ hasText: "(you)" })).toContainText("Ada 2");
  expect(new URL(second.url()).searchParams.get("seat")).toBe(seats[1]);

  // The host lets the server play the last seat, and every tab sees it at once.
  await players(page).getByRole("button", { name: "Rule bot", exact: true }).click();
  for (const tab of [page, second, third]) {
    await expect(rows(tab).nth(3)).toContainText("Rule bot 1");
    await expect(
      rows(tab)
        .nth(3)
        .getByRole("img", { name: /^Rule bot/ }),
    ).toBeVisible();
    await expect(players(tab)).toContainText("4 of 4");
  }
  await expect(note(second)).toHaveText("Waiting for Ada to start");
  // A seat that the server plays can be removed again, and added again.
  await players(page).getByRole("button", { name: "Remove Rule bot 1" }).click();
  await expect(players(second)).toContainText("3 of 4");
  await players(page).getByRole("button", { name: "Fill with rule bots" }).click();
  await expect(players(third)).toContainText("4 of 4");
  await expect(note(page)).toHaveText("");
  await expect(main(page)).toBeEnabled();
  await page.screenshot({ path: "shots/online/lobby-full.png", animations: "disabled" });

  // Enter is the main button: the game starts, and every tab mounts it with its player.
  await page.keyboard.press("Enter");
  for (const [index, tab] of [page, second, third].entries()) {
    await expect(tab.getByText("Round 1 · Strategy phase")).toBeVisible();
    await expect(tab.getByRole("region", { name: "Players" }).locator("tbody tr")).toHaveCount(4);
    // Each tab is another seat of the same game: the seat of its address, by its name.
    await expect(ownSeat(tab)).toHaveAttribute(
      "aria-label",
      new RegExp(`^${["Ada", "Ada 2", "Ada 3"][index]}, `),
    );
    // The names of the three tabs are in the player table of each, and the seat that the
    // server plays has its sign.
    for (const name of ["Ada", "Ada 2", "Ada 3", "Rule bot 1"]) {
      await expect(seatRows(tab).getByText(name, { exact: true })).toBeVisible();
    }
    await expect(seatRows(tab).getByRole("img", { name: "No person plays this seat" })).toHaveCount(
      1,
    );
    expect(new URL(tab.url()).searchParams.get("online")).toBe(game);
    expect(new URL(tab.url()).searchParams.get("seat")).toBe(seats[index]);
  }
  // A reload of a tab of the running game is the same seat.
  await third.reload();
  await expect(third.getByText("Round 1 · Strategy phase")).toBeVisible();
  await expect(ownSeat(third)).toHaveAttribute("aria-label", /^Ada 3, /);
  expect(errors).toEqual([]);
});

test("a visitor of another browser sees the table, joins with a name and is told what changes", async ({
  page,
  request,
}) => {
  const errors = errorsOf(page);
  const table = await makeTable(request, 3, "Ada");
  // The link of the table has no seat: this browser has no player there, so it is a visitor.
  await page.goto(`/?online=${table.id}`);
  await expect(title(page)).toHaveText("Lobby · 3 players");
  await expect(rows(page).nth(0)).toContainText("Ada");
  await expect(rows(page).filter({ hasText: "(you)" })).toHaveCount(0);
  await expect(note(page)).toHaveText("2 open seats");
  await expect(players(page).getByRole("button")).toHaveCount(1); // only "?"
  // The host has no socket open: one sign says so.
  await expect(rows(page).nth(0).getByRole("img", { name: "Host · not connected" })).toBeVisible();
  await page.screenshot({ path: "shots/online/lobby-visitor.png", animations: "disabled" });

  // Somebody else joins: the visitor sees it with no reload.
  await table.call("join", { kind: "new", nickname: "Blair" }, null);
  await expect(rows(page).nth(1)).toContainText("Blair");
  await expect(note(page)).toHaveText("1 open seat");

  // The visitor joins. A name is needed; Enter in the field is the main button.
  await expect(main(page)).toHaveText(/Join/);
  await page.getByLabel("Name").fill("");
  await expect(main(page)).toBeDisabled();
  await page.getByLabel("Name").fill("Cy");
  await page.getByLabel("Name").press("Enter");
  await expect(page).toHaveURL(/&seat=[^&]+$/);
  await expect(rows(page).nth(2)).toContainText("Cy (you)");
  // The table is full: nobody says "Ready", the host starts.
  await expect(note(page)).toHaveText("Waiting for Ada to start");
  await expect(main(page)).toHaveCount(0);
  expect(await page.evaluate(() => localStorage.getItem("ti4.nickname"))).toBe("Cy");

  // The host removes the player: the tab is a visitor again, and the browser forgets the seat.
  const seat = new URL(page.url()).searchParams.get("seat")!;
  await table.call("remove-bot", { player_id: seat });
  await expect(page).toHaveURL(new RegExp(`\\?online=${table.id}$`));
  await expect(note(page)).toHaveText("1 open seat");
  await expect(main(page)).toHaveText(/Join/);
  expect(
    await page.evaluate((key) => localStorage.getItem(key), `ti4.online.${table.id}.players`),
  ).toBe("{}");
  expect(errors).toEqual([]);
});

test("a tab with no seat is the one player of this browser, or asks which of several", async ({
  page,
  request,
}) => {
  const table = await makeTable(request, 4, "Ada");
  const joined = await table.call("join", { kind: "new", nickname: "Ada 2" }, null);
  // One player of this browser: the tab is that player, and the address gets its seat.
  await asPlayer(page, table.id, table.host.id, table.host.session);
  await page.goto(`/?online=${table.id}`);
  await expect(page).toHaveURL(new RegExp(`&seat=${table.host.id}$`));
  await expect(rows(page).nth(0)).toContainText("Ada (you)");

  // Two players of this browser: the list marks them, and a row makes the tab that player.
  await page.addInitScript(
    ([key, value]) => localStorage.setItem(key, value),
    [
      `ti4.online.${table.id}.players`,
      JSON.stringify({
        [table.host.id]: table.host.session,
        [joined.player.id]: joined.player_session,
      }),
    ],
  );
  await page.goto(`/?online=${table.id}`);
  await expect(rows(page).filter({ hasText: "(yours)" })).toHaveCount(2);
  await expect(rows(page).filter({ hasText: "(you)" })).toHaveCount(0);
  await expect(main(page)).toHaveText(/Join/);
  await rows(page).nth(1).getByRole("button", { name: /Ada 2/ }).click();
  await expect(page).toHaveURL(new RegExp(`&seat=${joined.player.id}$`));
  await expect(rows(page).nth(1)).toContainText("Ada 2 (you)");
  await expect(rows(page).nth(0)).toContainText("Ada (yours)");

  // A seat of the address that this browser does not have is no seat.
  await page.goto(`/?online=${table.id}&seat=somebody_else`);
  await expect(rows(page).filter({ hasText: "(yours)" })).toHaveCount(2);
  await expect(page).toHaveURL(new RegExp(`\\?online=${table.id}$`));
});

test("a call that the server refuses is a toast, and the lobby stays as it was", async ({
  page,
  request,
}) => {
  const table = await makeTable(request, 2, "Ada");
  await asPlayer(page, table.id, table.host.id, table.host.session);
  await page.goto(`/?online=${table.id}`);
  await expect(players(page)).toContainText("1 of 2");
  // The seat is taken behind the back of the page, and the answer of the server is late.
  await page.route("**/lobby/bots", async (route) => {
    await table.call("join", { kind: "new", nickname: "Blair" }, null);
    await route.continue();
  });
  await players(page).getByRole("button", { name: "Random", exact: true }).click();
  await expect(page.getByRole("status").filter({ hasText: "Seat is unavailable" })).toBeVisible();
  await expect(rows(page).nth(1)).toContainText("Blair");
  await expect(players(page)).toContainText("2 of 2");
});

test("a lost connection of the lobby is said in the header, and the table comes back by itself", async ({
  page,
  request,
}) => {
  const errors = errorsOf(page);
  const table = await makeTable(request, 3, "Ada");
  await asPlayer(page, table.id, table.host.id, table.host.session);
  let down = false;
  const open: { close(): Promise<void> }[] = [];
  await page.routeWebSocket(/\/ws\/tables\//, (socket) => {
    if (down) {
      return void socket.close();
    }
    socket.connectToServer();
    open.push(socket);
  });
  // The board is asked over HTTP, and its answer is late: it comes when the socket is gone.
  let board = () => {};
  const held = new Promise<void>((release) => {
    board = release;
  });
  await page.route("**/lobby/map-preview", async (route) => {
    await held;
    await route.continue();
  });
  await page.goto(`/?online=${table.id}`);
  await expect(players(page)).toContainText("1 of 3");
  down = true;
  await Promise.all(open.map((socket) => socket.close()));
  // One signal: the lobby stays in view.
  await expect(page.locator("header").getByRole("status")).toHaveText("Offline · reconnecting");
  await expect(rows(page)).toHaveCount(3);
  // An answer of a call says nothing about the socket: the page is still offline.
  board();
  await expect(page.locator("g.system.home")).toHaveCount(3);
  await expect(page.locator("header").getByRole("status")).toHaveText("Offline · reconnecting");
  // What changed meanwhile is there when the page is back.
  await table.call("bots", { kind: "random" });
  down = false;
  await expect(players(page)).toContainText("2 of 3");
  await expect(page.locator("header").getByRole("status")).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("the lobby of eight seats with the long names fits the screen", async ({ page, request }) => {
  const table = await makeTable(request, 8, LONG[0]);
  await table.call("join", { kind: "new", nickname: LONG[1] }, null);
  await table.call("bots", { kind: "smart", count: 3 });
  await table.call("bots", { kind: "random", count: 2 });
  await asPlayer(page, table.id, table.host.id, table.host.session);
  await page.goto(`/?online=${table.id}`);
  await expect(rows(page)).toHaveCount(8);
  await expect(page.locator("g.system.home")).toHaveCount(8);
  // The page does not scroll, and no row is wider than the panel.
  const size = await page.evaluate(() => ({
    width: document.documentElement.scrollWidth,
    height: document.documentElement.scrollHeight,
  }));
  expect(size).toEqual({ width: 1920, height: 1080 });
  const list = players(page).getByRole("list");
  expect(await list.evaluate((node) => node.scrollHeight <= node.clientHeight)).toBe(true);
  expect(await list.evaluate((node) => node.scrollWidth <= node.clientWidth)).toBe(true);
  // A long name is cut, and the full name is on hover.
  const name = rows(page).nth(0).getByTitle(LONG[0]);
  await expect(name).toContainText("(you)");
  expect(await name.evaluate((node) => node.scrollWidth > node.clientWidth)).toBe(true);
  await expect(main(page)).toBeInViewport();
  await page.screenshot({ path: "shots/online/lobby-eight.png", animations: "disabled" });

  // The choice of the faction opens in the row of the viewer: the page still does not scroll,
  // and the panel shows the whole list.
  await table.call("seat-faction", { player_id: table.host.id, faction: "hacan" });
  await factionOf(page, LONG[0]).click();
  const open = choice(page, LONG[0]);
  await expect(open.getByRole("button", { name: "Random" })).toBeInViewport();
  await expect(open.getByRole("region", { name: "Base game" }).getByRole("listitem")).toHaveCount(
    17,
  );
  const pok = open.getByRole("region", { name: "Prophecy of Kings" }).getByRole("listitem");
  await expect(pok).toHaveCount(7);
  await expect(pok.last()).toBeInViewport({ ratio: 1 });
  expect(
    await page.evaluate(() => ({
      width: document.documentElement.scrollWidth,
      height: document.documentElement.scrollHeight,
    })),
  ).toEqual({ width: 1920, height: 1080 });
  await page.screenshot({ path: "shots/online/lobby-faction.png", animations: "disabled" });
  // The sheet of a faction is behind "?": as printed, inside the window.
  await open.getByRole("button", { name: "About Naalu" }).click();
  const sheet = page.getByRole("note");
  await expect(sheet).toContainText("The Naalu Collective");
  await expect(sheet.getByRole("region", { name: "Leaders" })).toContainText("Unlock:");
  const box = (await sheet.boundingBox())!;
  expect(box.y).toBeGreaterThanOrEqual(0);
  expect(box.y + box.height).toBeLessThanOrEqual(1080);
  await page.screenshot({ path: "shots/online/lobby-faction-sheet.png", animations: "disabled" });
});

test("two tabs choose their factions, a taken faction is refused, and each plays what it chose", async ({
  page,
  context,
  request,
}) => {
  // Longer than a test of this suite should be: two tabs choose, and the game is started.
  test.setTimeout(10_000);
  const errors = errorsOf(page);
  const table = await makeTable(request, 4, "Ada");
  const blair = await table.call("join", { kind: "new", nickname: "Blair" }, null);
  await asPlayers(context, table.id, {
    [table.host.id]: table.host.session,
    [blair.player.id]: blair.player_session,
  });
  await page.goto(`/?online=${table.id}&seat=${table.host.id}`);
  const other = await context.newPage();
  // The table can be held back from the second tab, so that it chooses on what it saw last.
  let held = false;
  await other.routeWebSocket(/\/ws\/tables\//, (socket) => {
    const server = socket.connectToServer();
    server.onMessage((message) => held || socket.send(message));
  });
  await other.goto(`/?online=${table.id}&seat=${blair.player.id}`);

  // The row of the viewer is the control, and only it: Blair is not the host.
  await expect(factionOf(page, "Ada")).toHaveText(/Random/);
  await expect(factionOf(other, "Blair")).toHaveText(/Random/);
  await expect(factionOf(other, "Ada")).toHaveCount(0);
  await expect(factionOf(page, "Blair")).toHaveCount(0);

  // A choice applies at once, and the other tab sees it with no reload.
  await factionOf(page, "Ada").click();
  await expect(choice(page, "Ada").getByRole("button", { name: "Random" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await choice(page, "Ada").getByRole("button", { name: "Hacan", exact: true }).click();
  // A pick closes the choice.
  await expect(choice(page, "Ada")).toHaveCount(0);
  await expect(factionOf(page, "Ada")).toHaveText(/Hacan/);
  await expect(rows(other).nth(0)).toContainText("Hacan", { timeout: 2_000 });
  // The faction of another seat stays in its place and says who has it.
  await factionOf(other, "Blair").click();
  const taken = choice(other, "Blair").getByRole("button", { name: /^Hacan/ });
  await expect(taken).toHaveAttribute("aria-disabled", "true");
  await expect(taken).toContainText("Ada");
  await choice(other, "Blair").getByRole("button", { name: "Sol", exact: true }).click();
  await expect(factionOf(other, "Blair")).toHaveText(/Sol/);
  await expect(rows(page).nth(1)).toContainText("Sol", { timeout: 2_000 });
  // The sheet of the faction of another row is behind its name.
  await rows(page).nth(1).getByRole("button", { name: "About Sol" }).click();
  await expect(page.getByRole("note")).toContainText("The Federation of Sol");
  await page.keyboard.press("Escape");

  // Ada changes to Jol-Nar while the second tab is not told: its choice of Jol-Nar is refused
  // by the server, and the answer is a toast.
  held = true;
  await factionOf(page, "Ada").click();
  await choice(page, "Ada").getByRole("button", { name: "Jol-Nar", exact: true }).click();
  await expect(factionOf(page, "Ada")).toHaveText(/Jol-Nar/);
  await factionOf(other, "Blair").click();
  await choice(other, "Blair").getByRole("button", { name: "Jol-Nar", exact: true }).click();
  await expect(toast(other, "Jol-Nar is already taken by Ada")).toBeVisible();
  held = false;
  // Esc leaves the choice as it is.
  await factionOf(page, "Ada").click();
  await expect(choice(page, "Ada")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(choice(page, "Ada")).toHaveCount(0);
  await expect(factionOf(page, "Ada")).toHaveText(/Jol-Nar/);

  // The host chooses for a seat that the server plays; the other seat is left at "Random".
  await players(page).getByRole("button", { name: "Fill with rule bots" }).click();
  await expect(factionOf(other, "Blair")).toHaveText(/Sol/);
  await expect(factionOf(other, "Rule bot 1")).toHaveCount(0);
  await factionOf(page, "Rule bot 1").click();
  await choice(page, "Rule bot 1").getByRole("button", { name: "Naalu", exact: true }).click();
  await expect(rows(other).nth(2)).toContainText("Naalu", { timeout: 2_000 });
  await expect(rows(other).nth(3)).toContainText("Random");

  // The game: each tab plays the faction that it chose, and the seat that was left at "Random"
  // has a faction that no other seat has.
  await page.keyboard.press("Enter");
  for (const tab of [page, other]) {
    await expect(tab.getByText("Round 1 · Strategy phase")).toBeVisible();
  }
  await expect(ownSeat(page)).toHaveAttribute("aria-label", /Jol-Nar/);
  await expect(ownSeat(other)).toHaveAttribute("aria-label", /Sol/);
  const seats = page.getByRole("region", { name: "Players" }).locator("tbody tr th button");
  await expect(seats).toHaveCount(4);
  const factions = (await seats.evaluateAll((nodes) => nodes.map((node) => node.ariaLabel!))).map(
    // The label is the name of the player, then the faction.
    (label) => label.replace(/^[^,]+, /, "").replace(/[.,].*$/, ""),
  );
  expect(factions).toEqual(expect.arrayContaining(["Jol-Nar", "Sol", "Naalu"]));
  expect(new Set(factions).size).toBe(4);
  expect(errors).toEqual([]);
});

test("the host shuffles the seats: every tab sees the rows in their new order, and the game seats them so", async ({
  page,
  context,
  request,
}) => {
  const errors = errorsOf(page);
  const table = await makeTable(request, 4, "Ada");
  const blair = await table.call("join", { kind: "new", nickname: "Blair" }, null);
  await table.call("bots", { kind: "smart", count: 2 });
  await asPlayers(context, table.id, {
    [table.host.id]: table.host.session,
    [blair.player.id]: blair.player_session,
  });
  await page.goto(`/?online=${table.id}&seat=${table.host.id}`);
  const other = await context.newPage();
  const otherErrors = errorsOf(other);
  await other.goto(`/?online=${table.id}&seat=${blair.player.id}`);
  const NAMES = ["Ada", "Blair", "Rule bot 1", "Rule bot 2"];
  /** The players in the order of the rows. */
  const order = async (tab: Page) =>
    (await rows(tab).allInnerTexts()).map((text) => NAMES.find((name) => text.includes(name)));
  for (const tab of [page, other]) {
    await expect(rows(tab)).toHaveCount(4);
    expect(await order(tab)).toEqual(NAMES);
  }
  // Only the host has the button, beside the count of the players.
  await expect(players(other).getByRole("button", { name: "Shuffle seats" })).toHaveCount(0);
  await players(page).getByRole("button", { name: "Shuffle seats" }).click();
  await expect.poll(() => order(page)).not.toEqual(NAMES);
  const shuffled = await order(page);
  expect([...shuffled].sort()).toEqual(NAMES);
  await expect.poll(() => order(other)).toEqual(shuffled);
  await expect(toast(other, "The host shuffled the seats")).toBeVisible();

  // The rows are the seats of the game.
  await page.keyboard.press("Enter");
  for (const tab of [page, other]) {
    await expect(tab.getByText("Round 1 · Strategy phase")).toBeVisible();
  }
  const response = await request.get(`/api/games/${table.id}/snapshot`, {
    headers: { "x-ti4-player-session": table.host.session },
  });
  expect(response.ok()).toBe(true);
  const snapshot = await response.json();
  expect(
    snapshot.view.seating_order.map((id: string) => snapshot.seats[id].nickname as string),
  ).toEqual(shuffled);
  expect([...errors, ...otherErrors]).toEqual([]);
});

test("the host sets up the table, and the other tab is told at once", async ({
  page,
  context,
  request,
}) => {
  const errors = errorsOf(page);
  const table = await makeTable(request, 3, "Ada");
  const blair = await table.call("join", { kind: "new", nickname: "Blair" }, null);
  await asPlayers(context, table.id, {
    [table.host.id]: table.host.session,
    [blair.player.id]: blair.player_session,
  });
  await page.goto(`/?online=${table.id}&seat=${table.host.id}`);
  const other = await context.newPage();
  await other.goto(`/?online=${table.id}&seat=${blair.player.id}`);
  await expect(title(other)).toHaveText("Lobby · 3 players");
  // The same screen for a player who is not the host, with the values and no controls.
  await expect(setup(other).getByRole("button", { name: /player/ })).toHaveCount(0);
  await expect(setup(other).getByRole("combobox")).toHaveCount(0);
  await expect(said(other, "Strategy cards")).toHaveText("Thunder's Edge");

  // The players, in steps of one: a change applies at once, for every tab.
  await setup(page).getByRole("button", { name: "One player more" }).click();
  await expect(value(page, "Players")).toHaveText("4");
  await expect(title(other)).toHaveText("Lobby · 4 players");
  await expect(rows(other)).toHaveCount(4);
  await expect(toast(other, "The host set the players to 4")).toBeVisible();
  await expect(page.locator("g.system.home")).toHaveCount(4);
  // The next table of this browser has as many seats.
  expect(await page.evaluate(() => localStorage.getItem("ti4.players"))).toBe("4");
  // A seat that the server plays gives its seat, an open seat first; a person does not.
  await players(page).getByRole("button", { name: "Fill with rule bots" }).click();
  await expect(players(page)).toContainText("4 of 4");
  await setup(page).getByRole("button", { name: "One player fewer" }).click();
  await expect(rows(other)).toHaveCount(3);
  await setup(page).getByRole("button", { name: "One player fewer" }).click();
  await expect(rows(other)).toHaveCount(2);
  const fewer = setup(page).getByRole("button", { name: /^One player fewer/ });
  await expect(fewer).toHaveAttribute("aria-disabled", "true");
  await expect(fewer).toHaveAttribute("title", "A table has 2 seats at least");

  // The set of strategy cards, from the list of the server.
  await value(page, "Strategy cards").selectOption({ label: "Prophecy of Kings" });
  await expect(said(other, "Strategy cards")).toHaveText("Prophecy of Kings");
  await expect(toast(other, "The host changed the strategy cards")).toBeVisible();
  // What a set changes is behind "?": no card is named on the screen (the scenarios of "Start
  // from" have names such as "Strategy card: Construction", in a closed list).
  await expect(setup(page).getByText("Construction").and(page.locator(":not(option)"))).toHaveCount(
    0,
  );
  await setup(page).getByRole("button", { name: "About Strategy cards" }).click();
  await expect(
    page
      .getByRole("note")
      .getByText(/Construction/)
      .first(),
  ).toBeVisible();
  await page.keyboard.press("Escape");

  // The wait of the seats that the server plays, in steps of half a second.
  await expect(value(page, "Bots wait")).toHaveText("Off");
  await setup(page).getByRole("button", { name: "Wait more" }).click();
  await expect(value(page, "Bots wait")).toHaveText("0.5 s");
  await expect(said(other, "Bots wait")).toHaveText("0.5 s");

  // The name is edited in the row of the viewer. Esc leaves the editor.
  await rows(page).nth(0).getByRole("button", { name: "Change your name" }).click();
  await page.getByLabel("Your name").fill("Nobody");
  await page.keyboard.press("Escape");
  await expect(rows(page).nth(0)).toContainText("Ada (you)");
  await rows(page).nth(0).getByRole("button", { name: "Change your name" }).click();
  await page.getByLabel("Your name").fill("Grace");
  await page.keyboard.press("Enter");
  await expect(rows(page).nth(0)).toContainText("Grace (you)");
  await expect(rows(other).nth(0)).toContainText("Grace");
  expect(await page.evaluate(() => localStorage.getItem("ti4.nickname"))).toBe("Grace");
  // Enter in the name was not the main button: the table is still a lobby.
  await expect(main(page)).toHaveText(/Start game/);
  // A row of another player is no control.
  await expect(rows(other).getByRole("button", { name: "Change your name" })).toHaveCount(1);
  await expect(rows(other).nth(1).getByRole("button", { name: "Change your name" })).toBeVisible();
  await page.screenshot({ path: "shots/online/lobby-setup.png", animations: "disabled" });
  expect(errors).toEqual([]);
});

test("a secondary is drafted in one tab while a rule bot takes its time, and the toolbar says who the game waits for", async ({
  page,
  context,
  request,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = errorsOf(page);
  const table = await makeTable(request, 4, "Ada");
  await asPlayers(context, table.id, { [table.host.id]: table.host.session });
  await page.goto(`/?online=${table.id}&seat=${table.host.id}`);
  // The host starts from the late game with deflectors. It is a game of its own: eight seats,
  // its map, its cards and its factions, so these rows have no control. Its seed and the places of
  // the seats tell what each rule bot does: in this game none cancels the strategic action of the
  // host.
  await value(page, "Start from").selectOption({ label: "Late game with deflectors" });
  await expect(title(page)).toHaveText("Lobby · 8 players");
  await expect(said(page, "Players")).toHaveText("8");
  await expect(setup(page).getByRole("combobox")).toHaveCount(1);
  await expect(rows(page).nth(0)).toContainText("Sol");
  // Two rule bots, a second person, and four more rule bots: the seats are in this order.
  await table.call("bots", { kind: "smart", count: 2 });
  const blair = await table.call("join", { kind: "new", nickname: "Blair" }, null);
  await table.call("bots", { kind: "smart", count: 4 });
  await asPlayers(context, table.id, {
    [table.host.id]: table.host.session,
    [blair.player.id]: blair.player_session,
  });
  const other = await context.newPage();
  const otherErrors = errorsOf(other);
  await other.goto(`/?online=${table.id}&seat=${blair.player.id}`);
  await expect(said(other, "Start from")).toHaveText("Late game with deflectors");
  // The seats that the server plays wait a second before each answer.
  await setup(page).getByRole("button", { name: "Wait more" }).click();
  await setup(page).getByRole("button", { name: "Wait more" }).click();
  await expect(value(page, "Bots wait")).toHaveText("1 s");
  await expect(said(other, "Bots wait")).toHaveText("1 s");
  await expect(toast(other, "The host set the wait of the bots to 1 s")).toBeVisible();
  await page.keyboard.press("Enter");
  for (const tab of [page, other]) {
    await expect(tab.getByText("Round 4 · Action phase")).toBeVisible();
  }

  // The host has the turn, as the first seat of the scenario, and plays Leadership.
  await panel(page)
    .getByRole("button", { name: /^1 Leadership$/ })
    .click();
  await send(page).click();
  // The other tab drafts its secondary from the moment the card is played.
  await expect(panel(other).getByRole("heading", { name: "Leadership" })).toBeVisible();
  await expect(panel(other).getByText("✎ Private draft")).toBeVisible();
  await expect(title(other)).toHaveText("Waiting for Ada");
  // The gained tokens go to the tactic pool, and nothing is bought: one screen, one plan.
  for (let token = 0; token < 3; token++) {
    await panel(page).getByRole("button", { name: "Add tactic pool token" }).click();
  }
  await send(page).click();
  // The first seat after the host is a rule bot. It takes a second for each of its answers, and
  // every tab reads who the game waits for.
  for (const tab of [page, other]) {
    await expect(title(tab)).toHaveText("Waiting for Rule bot 1");
    await expect(panel(tab).getByText("▶ Deciding")).toHaveCount(1);
  }
  // Meanwhile the draft is made and marked ready. With no wait the game would be past this
  // seat before the first click.
  await panel(other).getByRole("button", { name: "Spend nothing further" }).click();
  await expect(send(other)).toHaveText(/^Record/);
  await send(other).click();
  await expect(send(other)).toHaveText(/^Mark draft ready/);
  await send(other).click();
  await expect(panel(other).getByText("✎ Draft ready")).toBeVisible();
  await expect(title(other)).toHaveText("Waiting for Rule bot 1");
  // The draft is private: the host sees the row of that seat wait, and nothing of the draft.
  await expect(panel(page).getByText("✎")).toHaveCount(0);
  await other.screenshot({ path: "shots/online/secondary-draft-wait.png", animations: "disabled" });
  expect([...errors, ...otherErrors]).toEqual([]);
});

test("a ready secondary of a seat in two tabs is sent once, by the server, with no toast", async ({
  page,
  context,
  request,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const table = await makeTable(request, 4, "Ada");
  // Blair is the second seat: the first to follow, and the next to have a turn. In the late game
  // with deflectors no rule bot cancels the strategic action of the host.
  await table.call("settings", { start_from: "late-game-deflectors" });
  const blair = await table.call("join", { kind: "new", nickname: "Blair" }, null);
  await table.call("bots", { kind: "smart", fill: true });
  await asPlayers(context, table.id, {
    [table.host.id]: table.host.session,
    [blair.player.id]: blair.player_session,
  });
  await page.goto(`/?online=${table.id}&seat=${table.host.id}`);
  // Blair has the game open in two tabs. What the tabs answer is counted.
  const tabs = [await context.newPage(), await context.newPage()];
  const errors = [page, ...tabs].map(errorsOf);
  const answered: string[] = [];
  for (const tab of tabs) {
    tab.on("websocket", (socket) =>
      socket.on("framesent", ({ payload }) => {
        const { type } = JSON.parse(String(payload));
        if (type === "submit_choice" || type === "apply_draft") {
          answered.push(type);
        }
      }),
    );
    await tab.goto(`/?online=${table.id}&seat=${blair.player.id}`);
  }
  await expect(said(tabs[0], "Start from")).toHaveText("Late game with deflectors");
  await expect(title(page)).toHaveText("Lobby · 8 players");
  await page.keyboard.press("Enter");
  for (const tab of [page, ...tabs]) {
    await expect(tab.getByText("Round 4 · Action phase")).toBeVisible();
  }
  const [one, two] = tabs;

  // The host plays Leadership. One tab drafts the secondary and marks it ready while the host
  // resolves the primary; the other tab has the same draft.
  await panel(page)
    .getByRole("button", { name: /^1 Leadership$/ })
    .click();
  await send(page).click();
  await expect(panel(one).getByText("✎ Private draft")).toBeVisible();
  await panel(one).getByRole("button", { name: "Spend nothing further" }).click();
  await expect(send(one)).toHaveText(/^Record/);
  await send(one).click();
  await expect(send(one)).toHaveText(/^Mark draft ready/);
  await send(one).click();
  await expect(panel(two).getByText("✎ Draft ready")).toBeVisible();
  // The gained tokens go to the tactic pool, and nothing is bought: one screen, one plan.
  for (let token = 0; token < 3; token++) {
    await panel(page).getByRole("button", { name: "Add tactic pool token" }).click();
  }
  await send(page).click();

  // The game comes to Blair: the server sends the draft, and the seat is not asked. The
  // strategic action is over with no answer of a tab, and no tab is told that the game went on
  // before its answer.
  await expect(
    panel(page).getByRole("heading", { name: "End your turn", exact: true }),
  ).toBeVisible();
  for (const tab of tabs) {
    await expect(panel(tab).getByText("✎ Draft ready")).toHaveCount(0);
    await expect(tab.getByText("Your draft no longer fits the game. It is not ready.")).toHaveCount(
      0,
    );
    await expect(
      tab.getByRole("status").filter({ hasText: /Nothing was sent|another tab/ }),
    ).toHaveCount(0);
  }
  expect(answered).toEqual([]);
  expect(errors.flat()).toEqual([]);
});

test("a table starts from the late game, and each tab has the seat of its join", async ({
  page,
  context,
  request,
}) => {
  const errors = errorsOf(page);
  const table = await makeTable(request, 4, "Ada");
  await table.call("settings", { start_from: "late-game" });
  const blair = await table.call("join", { kind: "new", nickname: "Blair" }, null);
  await table.call("bots", { kind: "smart", fill: true });
  await asPlayers(context, table.id, {
    [table.host.id]: table.host.session,
    [blair.player.id]: blair.player_session,
  });
  await page.goto(`/?online=${table.id}&seat=${table.host.id}`);
  const other = await context.newPage();
  await other.goto(`/?online=${table.id}&seat=${blair.player.id}`);
  // Every player is told where the game starts, and the seats have the factions of the scenario.
  await expect(said(other, "Start from")).toHaveText("Late game");
  await expect(rows(other).nth(0)).toContainText("Sol");
  await expect(rows(other).nth(1)).toContainText("Blair (you)");
  await expect(rows(other).nth(1)).toContainText("Hacan");
  await expect(rows(other).filter({ hasText: "Random" })).toHaveCount(0);
  await page.keyboard.press("Enter");
  for (const tab of [page, other]) {
    await expect(tab.getByText("Round 4 · Action phase")).toBeVisible();
    await expect(tab.getByRole("region", { name: "Players" }).locator("tbody tr")).toHaveCount(8);
  }
  // The seats are not drawn: the host is the first seat, the second person the second.
  await expect(ownSeat(page)).toHaveAttribute("aria-label", /Sol/);
  await expect(ownSeat(other)).toHaveAttribute("aria-label", /Hacan/);
  await expect(title(page)).toHaveText("Your decision");
  await expect(title(other)).toHaveText("Waiting for Ada");
  // A sheet of the late game in each tab: each seat has its own hand of the scenario.
  for (const tab of [page, other]) {
    await tab.getByRole("button", { name: "Secrets", exact: true }).click();
    const mine = tab
      .getByRole("complementary", { name: "Secrets" })
      .getByRole("region", { name: "Your secret objectives" });
    await expect(mine.getByRole("article")).toHaveCount(3);
  }
  await page.getByRole("button", { name: "Objectives", exact: true }).click();
  await expect(page.getByRole("complementary", { name: "Objectives" })).toContainText(
    "10 VP to win",
  );
  expect(errors).toEqual([]);
});

/** A started table of four: Ada (the host), a person with a long name, a rule bot and a random seat. */
async function runningTable(request: APIRequestContext) {
  const table = await makeTable(request, 4, "Ada");
  const guest = await table.call("join", { kind: "new", nickname: LONG[0] }, null);
  await table.call("bots", { kind: "smart" });
  await table.call("bots", { kind: "random" });
  await table.call("start", {});
  return {
    ...table,
    guest: { id: guest.player.id as string, session: guest.player_session as string },
  };
}

test("the table of a running game: every tab has the names, and the host sets the wait in the sheet", async ({
  page,
  context,
  request,
}) => {
  const errors = errorsOf(page);
  const table = await runningTable(request);
  await asPlayers(context, table.id, {
    [table.host.id]: table.host.session,
    [table.guest.id]: table.guest.session,
  });
  await page.goto(`/?online=${table.id}&seat=${table.host.id}`);
  const other = await context.newPage();
  const otherErrors = errorsOf(other);
  await other.goto(`/?online=${table.id}&seat=${table.guest.id}`);
  for (const tab of [page, other]) {
    await expect(tab.getByText("Round 1 · Strategy phase")).toBeVisible();
    await expect(seatRows(tab)).toHaveCount(4);
    // A seat is named by its player. A long name is cut, with the full name on hover.
    for (const name of ["Ada", LONG[0], "Rule bot 1", "Random bot 1"]) {
      await expect(seatRows(tab).getByTitle(name, { exact: true })).toBeVisible();
    }
    await expect(seatRows(tab).getByRole("img", { name: "No person plays this seat" })).toHaveCount(
      2,
    );
  }
  await expect(ownSeat(page)).toHaveAttribute("aria-label", /^Ada, /);
  await expect(ownSeat(other)).toHaveAttribute("aria-label", new RegExp(`^${LONG[0]}, `));
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(1920);

  // The sheet "Table" is the lobby with nothing open: the seats in their order, each with the
  // symbol of its seat, its player and its faction, and the setup.
  await page.getByRole("button", { name: "Table", exact: true }).click();
  const seats = sheet(page).getByRole("region", { name: "Players" }).getByRole("listitem");
  await expect(seats).toHaveCount(4);
  await expect(seats.filter({ hasText: "Ada (you)" })).toHaveCount(1);
  await expect(
    seats.filter({ hasText: "Ada (you)" }).getByRole("img", { name: "Host" }),
  ).toBeVisible();
  await expect(
    seats.filter({ hasText: "Rule bot 1" }).getByRole("img", { name: /^Rule bot/ }),
  ).toBeVisible();
  // Every seat has the faction that the start gave it: none is "Random" any more.
  await expect(seats.filter({ hasText: "Random bot 1" })).toHaveCount(1);
  await expect(seats.filter({ hasText: /Random$/ })).toHaveCount(0);
  const setupOf = (tab: Page) => sheet(tab).getByRole("region", { name: "Setup" });
  await expect(setupOf(page)).toContainText("Strategy cards");
  await expect(setupOf(page)).toContainText("Thunder’s Edge".replace("’", "'"));
  // Nothing of the table is edited there but the wait, and only by the host.
  await expect(sheet(page).getByRole("combobox")).toHaveCount(0);
  await expect(
    sheet(page).getByRole("button", { name: /Remove|Change your name|Faction of/ }),
  ).toHaveCount(0);
  await expect(setupOf(page).getByLabel("Bots wait", { exact: true })).toHaveText("Off");
  await setupOf(page).getByRole("button", { name: "Wait more" }).click();
  await expect(setupOf(page).getByLabel("Bots wait", { exact: true })).toHaveText("0.5 s");
  await page.screenshot({ path: "shots/online/table-sheet.png", animations: "disabled" });

  // The other player reads the same table, with the new wait and no control.
  await other.getByRole("button", { name: "Table", exact: true }).click();
  await expect(
    sheet(other).getByRole("region", { name: "Players" }).getByRole("listitem"),
  ).toHaveCount(4);
  await expect(setupOf(other)).toContainText("0.5 s");
  await expect(setupOf(other).getByRole("button", { name: "Wait more" })).toHaveCount(0);
  // Esc closes the sheet, as every reference sheet.
  await page.keyboard.press("Escape");
  await expect(sheet(page)).toHaveCount(0);
  expect([...errors, ...otherErrors]).toEqual([]);
});

test("a visitor of a running game is shown its table first: Watch opens the game, and a seat of a person who is away is offered", async ({
  page,
  request,
}) => {
  const errors = errorsOf(page);
  const table = await runningTable(request);
  // The grace time of a server is half a minute. The table of this page says that it is over
  // for the second person; the server still decides, and refuses.
  await page.routeWebSocket(/\/ws\/tables\//, (socket) => {
    const server = socket.connectToServer();
    server.onMessage((data) => {
      const message = JSON.parse(String(data));
      for (const slot of message.lobby?.slots ?? []) {
        slot.can_take_over = slot.occupant === table.guest.id;
      }
      socket.send(JSON.stringify(message));
    });
  });
  await page.goto(`/?online=${table.id}`);
  // The same screen as the lobby, with nothing to set up: the board, the seats, the setup.
  await expect(title(page)).toHaveText("Table · 4 players");
  await expect(rows(page)).toHaveCount(4);
  await expect(page.locator("g.system.home")).toHaveCount(4);
  await expect(rows(page).filter({ hasText: "(you)" })).toHaveCount(0);
  await expect(setup(page).getByRole("button", { name: /Wait/ })).toHaveCount(0);
  await expect(setup(page).getByRole("combobox")).toHaveCount(0);
  await expect(page.getByLabel("Name")).toHaveCount(0);
  await expect(main(page)).toHaveText(/Watch/);
  await page.screenshot({ path: "shots/online/table-visitor.png", animations: "disabled" });

  // Only the seat of the person who is away is offered. The server has the last word.
  const rejoin = players(page).getByRole("button", { name: "Rejoin", exact: true });
  await expect(rejoin).toHaveCount(1);
  await expect(
    rows(page).filter({ has: page.getByRole("button", { name: "Rejoin", exact: true }) }),
  ).toContainText(LONG[0]);
  await rejoin.click();
  await expect(toast(page, "Takeover is not available yet")).toBeVisible();
  await expect(page).toHaveURL(new RegExp(`\\?online=${table.id}$`));

  // Enter is the main button: the visitor watches the game, with no seat.
  await page.keyboard.press("Enter");
  await expect(page.getByText("Round 1 · Strategy phase")).toBeVisible();
  await expect(seatRows(page)).toHaveCount(4);
  await expect(ownSeat(page)).toHaveCount(0);
  await expect(seatRows(page).getByTitle("Ada", { exact: true })).toBeVisible();
  // The refusal of the server is the one error of the page: the browser logs the answer.
  expect(errors).toEqual([
    "Failed to load resource: the server responded with a status of 409 (Conflict)",
  ]);
});
