import { expect, test, type APIRequestContext, type Page } from "@playwright/test";

// The lobby of a table on a Pixel 9 in portrait: the panes of the game shell, Map · Players ·
// Setup, with the footer under every pane.

const LONG = [
  "Bartholomew Featherstonehaugh the Third of Mecatol",
  "Wilhelmina Vanderbilt-Montgomery",
];

const pane = (page: Page, name: string) =>
  page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });
const main = (page: Page) => page.locator('button[aria-keyshortcuts="Enter"]');
const fits = async (page: Page) =>
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(412);

/** A table of eight on the server: the two long names, three rule bots, two random seats, one open. */
async function tableOfEight(page: Page, request: APIRequestContext) {
  const response = await request.post("/api/games", {
    data: { player_count: 8, seed: 11, nickname: LONG[0] },
  });
  const made = await response.json();
  const call = async (path: string, data: object, session?: string) =>
    expect(
      (
        await request.post(`/api/games/${made.game_id}/lobby/${path}`, {
          data,
          headers: session ? { "x-ti4-player-session": session } : {},
        })
      ).ok(),
    ).toBe(true);
  await call("join", { kind: "new", nickname: LONG[1] });
  await call("bots", { kind: "smart", count: 3 }, made.player_session);
  await call("bots", { kind: "random", count: 2 }, made.player_session);
  await page.addInitScript(
    ([key, value]) => localStorage.setItem(key, value),
    [
      `ti4.online.${made.game_id}.players`,
      JSON.stringify({ [made.player.id]: made.player_session }),
    ],
  );
  await page.goto(`/?online=${made.game_id}`);

  return made.game_id as string;
}

test("phone: the start screen and the lobby of eight seats with the long names", async ({
  page,
  request,
}) => {
  await page.goto("/");
  await expect(page.getByRole("link", { name: /Multiplayer/ })).toBeVisible();
  await fits(page);
  await page.screenshot({ path: "shots/phone/start.png", animations: "disabled" });

  const game = await tableOfEight(page, request);

  // The Players pane is first: nothing is cut at the side, and a long name has a second line.
  const rows = page.getByRole("region", { name: "Players" }).locator("ul.overflow-y-auto > li");
  await expect(rows).toHaveCount(8);
  await expect(rows.nth(0)).toContainText(`${LONG[0]} (you)`);
  await fits(page);
  for (const row of await rows.all()) {
    const box = (await row.boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(412);
  }
  const name = rows.nth(0).getByTitle(LONG[0]);
  expect(await name.evaluate((node) => node.scrollWidth <= node.clientWidth)).toBe(true);
  // The main button is under every pane.
  await expect(main(page)).toHaveText("Start game");
  await expect(main(page)).toBeInViewport();
  await page.screenshot({ path: "shots/phone/online/lobby-players.png", animations: "disabled" });

  await pane(page, "Setup").tap();
  // The host has the controls of the setup: a counter is under a finger.
  const setup = page.getByRole("region", { name: "Setup" });
  await expect(setup.getByLabel("Players", { exact: true })).toHaveText("8");
  await expect(setup.getByLabel("Strategy cards", { exact: true })).toHaveValue("te");
  await setup.getByRole("button", { name: "Wait more" }).tap();
  await expect(setup.getByLabel("Bots wait", { exact: true })).toHaveText("0.5 s");
  await expect(main(page)).toBeInViewport();
  await fits(page);
  await page.screenshot({ path: "shots/phone/online/lobby-setup.png", animations: "disabled" });

  await pane(page, "Map").tap();
  await expect(page.locator("g.system.home")).toHaveCount(8);
  await expect(main(page)).toBeInViewport();
  await fits(page);
  await page.screenshot({ path: "shots/phone/online/lobby-map.png", animations: "disabled" });

  // A visitor has the name and "Join" in the footer, in reach on every pane.
  await page.addInitScript((key) => localStorage.removeItem(key), `ti4.online.${game}.players`);
  await page.goto(`/?online=${game}`);
  await expect(main(page)).toHaveText("Join");
  await expect(page.getByLabel("Name")).toBeInViewport();
  await expect(main(page)).toBeInViewport();
  await fits(page);
  await page.screenshot({ path: "shots/phone/online/lobby-visitor.png", animations: "disabled" });
});

test("phone: the choice of the faction and the sheet of a faction", async ({ page, request }) => {
  await tableOfEight(page, request);

  // The choice of the faction opens as a sheet over the pane, in two columns: nothing is cut at
  // the side, and the sheet has the room of the footer, so the main button is not shown.
  const players = page.getByRole("region", { name: "Players" });
  const opener = players.getByRole("button", { name: `Faction of ${LONG[0]}: Random` });
  await opener.tap();
  const choice = page.getByRole("group", { name: `Faction of ${LONG[0]}` });
  await expect(players.getByRole("group", { name: `Faction of ${LONG[0]}` })).toHaveCount(0);
  await expect(choice.getByRole("listitem")).toHaveCount(25);
  await fits(page);
  for (const option of await choice.getByRole("listitem").all()) {
    const box = (await option.boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(412);
  }
  await expect(main(page)).toBeHidden();
  await page.screenshot({ path: "shots/phone/online/lobby-faction.png", animations: "disabled" });
  // A tap on "?" keeps the sheet of a faction open, inside the screen.
  await choice.getByRole("button", { name: "About Sol" }).tap();
  const sheet = page.getByRole("note");
  await expect(sheet).toContainText("The Federation of Sol");
  const box = (await sheet.boundingBox())!;
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width).toBeLessThanOrEqual(412);
  expect(box.y).toBeGreaterThanOrEqual(0);
  expect(box.y + box.height).toBeLessThanOrEqual(923);
  await page.screenshot({
    path: "shots/phone/online/lobby-faction-sheet.png",
    animations: "disabled",
  });
  await choice.getByRole("button", { name: "About Sol" }).tap();
  // A tap on a row is the choice, at once: the sheet closes, and the control of the row says it.
  await choice.getByRole("button", { name: "Sol", exact: true }).tap();
  await expect(choice).toHaveCount(0);
  await expect(players.getByRole("button", { name: `Faction of ${LONG[0]}: Sol` })).toBeVisible();
  await expect(main(page)).toBeInViewport();
  // The "×" of the sheet leaves the choice as it is.
  await players.getByRole("button", { name: `Faction of ${LONG[0]}: Sol` }).tap();
  await expect(choice).toBeVisible();
  await page.getByRole("button", { name: `Close Faction of ${LONG[0]}` }).tap();
  await expect(choice).toHaveCount(0);
  await expect(main(page)).toBeInViewport();
});

test("phone: a visitor of a running game has its table on the panes, and the game has the sheet", async ({
  page,
  request,
}) => {
  // A started table of eight: the two long names, and six seats that the server plays.
  const response = await request.post("/api/games", {
    data: { player_count: 8, seed: 11, nickname: LONG[0] },
  });
  const made = await response.json();
  const call = async (path: string, data: object, session?: string) =>
    expect(
      (
        await request.post(`/api/games/${made.game_id}/lobby/${path}`, {
          data,
          headers: session ? { "x-ti4-player-session": session } : {},
        })
      ).ok(),
    ).toBe(true);
  await call("join", { kind: "new", nickname: LONG[1] });
  await call("bots", { kind: "smart", count: 3 }, made.player_session);
  await call("bots", { kind: "random", count: 3 }, made.player_session);
  await call("start", {}, made.player_session);
  // The second person is away for this page: its seat is offered.
  await page.routeWebSocket(/\/ws\/tables\//, (socket) => {
    const server = socket.connectToServer();
    server.onMessage((data) => {
      const message = JSON.parse(String(data));
      for (const slot of message.lobby?.slots ?? []) {
        slot.can_take_over = slot.nickname === LONG[1];
      }
      socket.send(JSON.stringify(message));
    });
  });
  await page.goto(`/?online=${made.game_id}`);

  // The table first, on the panes of the lobby, with "Watch" under every pane.
  await expect(page.locator("header strong").first()).toHaveText("Table · 8 players");
  const seats = page.getByRole("region", { name: "Players" }).locator("ul > li");
  await expect(seats).toHaveCount(8);
  await expect(seats.getByRole("button", { name: "Rejoin", exact: true })).toHaveCount(1);
  await expect(main(page)).toHaveText(/Watch/);
  await fits(page);
  await page.screenshot({ path: "shots/online/phone-table-visitor.png", animations: "disabled" });
  await pane(page, "Setup").click();
  await expect(page.getByRole("region", { name: "Setup" })).toContainText("Strategy cards");
  await expect(main(page)).toBeVisible();
  await fits(page);

  // The game, with no seat, and its table as a sheet of the menu: the names are not cut.
  await main(page).click();
  await expect(page.getByText("Round 1 · Strategy phase")).toBeVisible();
  // The status names a player with a long name: it is cut, and the menu stays in reach.
  await fits(page);
  await page.getByRole("button", { name: "Reference and more" }).click();
  await page.getByRole("menuitem", { name: "Table" }).click();
  const sheet = page.getByRole("complementary", { name: "Table" });
  await expect(sheet.getByRole("region", { name: "Players" }).getByRole("listitem")).toHaveCount(8);
  await expect(sheet).toContainText("Bots wait");
  await fits(page);
  await page.screenshot({ path: "shots/online/phone-table-sheet.png", animations: "disabled" });
});
