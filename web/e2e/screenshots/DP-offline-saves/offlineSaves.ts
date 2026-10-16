import { expect, type Page } from "@playwright/test";
import { playUntil } from "../../offlineDriver";
import { requireEngine } from "../DO-offline-game/offlineGame";

export { SIZES } from "../DO-offline-game/offlineGame";

/** What the build footer and the strip name: the first 8 hex digits of the engine file's sha-256. */
export const ENGINE_LABEL = /^engine [0-9a-f]{8}$/;

/** Blocks every call to a server, like a device with no connection. /health says it is the public build. */
export async function blockServer(page: Page, options: { health?: boolean } = {}) {
  await page.route(/\/(api|ws|advisor|health)(\/|$)/, (route) => {
    if (options.health && route.request().url().endsWith("/health"))
      return route.fulfill({ json: { status: "ok", protocol_version: 3, commit: "51390594", public: true } });
    return route.abort();
  });
}

/** Starts a real game (real engine in a Web Worker) from the setup form; returns its id. */
export async function newGame(
  page: Page,
  { players, seat, seed, nickname = "Ada" }: { players: number; seat: number; seed: number; nickname?: string },
): Promise<string> {
  requireEngine();
  await page.goto("/offline");
  await page.getByTestId("offline-nickname").fill(nickname);
  await page.getByTestId("offline-player-count").selectOption(String(players));
  await page.getByTestId("offline-seat").selectOption(String(seat));
  await page.getByTestId("offline-seed").fill(String(seed));
  await page.getByTestId("offline-start").click();
  await expect(page.getByTestId("offline-game")).toBeVisible({ timeout: 60_000 });
  const id = /\/offline\/games\/([^/?#]+)/.exec(page.url())?.[1];
  if (!id) throw new Error(`not on a game page: ${page.url()}`);
  return decodeURIComponent(id);
}

/** Three real games of different size and length, saved in this browser's IndexedDB. */
export async function threeGames(page: Page): Promise<string[]> {
  const ids: string[] = [];
  for (const [players, seat, seed, answers] of [
    [4, 3, 11, 14],
    [3, 1, 23, 6],
    [5, 2, 37, 10],
  ] as const) {
    const id = await newGame(page, { players, seat, seed });
    await playUntil(page, id, answers);
    await page.waitForTimeout(400); // the last write settles
    ids.push(id);
  }
  return ids;
}

/** Makes the stored game look as if another build of the engine had saved it. */
export async function pretendOtherEngine(page: Page, gameId: string) {
  await page.evaluate(
    (id) =>
      new Promise<void>((done, fail) => {
        const open = indexedDB.open("ti4-offline");
        open.onerror = () => fail(new Error("open"));
        open.onsuccess = () => {
          const db = open.result;
          const tx = db.transaction("games", "readwrite");
          const store = tx.objectStore("games");
          const get = store.get(id);
          get.onsuccess = () => {
            store.put({
              ...get.result,
              engine: { ...get.result.engine, wasmSha256: "5b1c9e07".padEnd(64, "a"), build: "0.0.9" },
            });
          };
          tx.oncomplete = () => {
            db.close();
            done();
          };
          tx.onerror = () => fail(new Error("write"));
        };
      }),
    gameId,
  );
}

/** Folds a decision dialog away, so the strip above the game and the board are in the picture. */
export async function foldDecision(page: Page) {
  const fold = page.locator('[data-testid="minimize-choice-button"], button[aria-label="Minimize decision"]').first();
  for (let i = 0; i < 3 && (await fold.count()) > 0; i++) {
    await fold.click({ timeout: 2_000 }).catch(() => undefined);
    await page.waitForTimeout(250);
  }
}

/** Scrolls "My offline games" to the top of the picture (the setup form sits above it). */
export async function showGamesList(page: Page) {
  await page.getByRole("heading", { name: "My offline games" }).evaluate((el) => el.scrollIntoView({ block: "start" }));
  await page.waitForTimeout(150);
}
