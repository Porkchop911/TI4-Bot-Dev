import { expect, type Page } from "@playwright/test";

const now = Math.floor(Date.now() / 1000);
const GAMES = [
  { game_id: "game_7c1f02aa", phase: "lobby", players: 2, created_at: now - 9 * 60 },
  { game_id: "game_b94d51e0", phase: "started", players: 4, created_at: now - 3 * 3600 },
  { game_id: "game_e20a7731", phase: "lobby", players: 1, created_at: now - 26 * 3600 },
];

/** The create form against a server that refuses with 429 too_many_games until games are ended. */
export async function openLimitDialog(page: Page) {
  const open = [...GAMES];
  await page.route("**/api/health", (route) => route.fulfill({ json: { status: "ok" } }));
  await page.route("**/api/games/*/terminate", (route) => {
    const id = route.request().url().split("/").slice(-2)[0];
    const at = open.findIndex((g) => g.game_id === id);
    if (at >= 0) open.splice(at, 1);
    return route.fulfill({ json: { terminated: true } });
  });
  await page.route("**/api/games", (route) =>
    route.request().method() === "POST"
      ? route.fulfill({
          status: 429,
          json: {
            error: "too_many_games",
            message: "you already have several unfinished games on this server",
            games: open,
          },
        })
      : route.fulfill({ json: open }),
  );
  // This browser made the first two games; the third was made from another browser.
  await page.addInitScript(() => {
    localStorage.setItem(
      "ti4.my-games",
      JSON.stringify({
        game_7c1f02aa: { gameId: "game_7c1f02aa", token: "tok1", storedAt: 0 },
        game_b94d51e0: { gameId: "game_b94d51e0", token: "tok2", storedAt: 0 },
      }),
    );
  });
  await page.goto("/");
  await page.getByLabel("Nickname").fill("Alex");
  await page.getByTestId("create-game-button").click();
  await expect(page.getByTestId("mygames-dialog")).toBeVisible();
}

export const rows = (page: Page) => page.getByTestId("mygames-row");
