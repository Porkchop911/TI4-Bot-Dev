import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { spawnTestServer, type TestServer } from "../../web-shared/test/server.ts";
import { NICKNAME, clickOnce, playUntil, startOfflineGame, storedAnswers, storedFeedback, storedRecord, storedSetup } from "./offlineDriver";

// The offline game, end to end, with the production build only (see offline.config.ts): the
// engine runs in a Web Worker (JSPI), and nothing but the built static files is ever fetched.

const baseOrigin = (page: Page) => new URL(page.url() || "http://127.0.0.1").origin;

/** Only the built files are served; every call to a server (and anything off-origin) fails. */
async function blockServer(context: BrowserContext, origin: string, requests: string[] = []) {
  await context.route("**/*", (route) => {
    const url = new URL(route.request().url());
    if (url.origin !== origin || /^\/(api|ws|health)(\/|$)/.test(url.pathname)) return route.abort();
    requests.push(`${route.request().method()} ${url.pathname}`);
    return route.continue();
  });
}

test("plays an offline game with the network blocked and resumes after a refresh", async ({ page, context, baseURL }) => {
  const origin = new URL(baseURL!).origin;
  const requests: string[] = [];
  await blockServer(context, origin, requests);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));

  const gameId = await startOfflineGame(page, 4);
  await expect(page.getByTestId("offline-banner")).toBeVisible();
  // The banner names the engine file (first 8 hex of its sha-256).
  await expect(page.getByTestId("offline-engine")).toHaveText(/^engine [0-9a-f]{8}$/);
  // The engine and its worker are loaded now: cut the network entirely.
  await context.setOffline(true);

  const played = await playUntil(page, gameId, 20);
  expect(played).toBeGreaterThanOrEqual(20);
  const record = await storedRecord(page, gameId);
  expect(record?.wasmSha256).toMatch(/^[0-9a-f]{64}$/);

  // Refresh: the page comes back from the browser cache of the built files; the game from the store.
  await context.setOffline(false);
  const before = await storedAnswers(page, gameId);
  await page.reload();
  await page.waitForSelector('[data-testid="offline-game"]', { timeout: 60_000 });
  expect(page.url()).toContain(`/offline/games/${gameId}`);
  expect(await storedAnswers(page, gameId)).toBe(before);
  // And it plays on from there.
  await context.setOffline(true);
  await playUntil(page, gameId, before + 5);

  // Nothing but static files of the build was requested.
  const unexpected = requests.filter((r) => !/^GET \/(assets\/|offline|$)/.test(r));
  expect(unexpected).toEqual([]);
  expect(requests.some((r) => /\.wasm$/.test(r))).toBe(true);
  expect(errors).toEqual([]);
  expect(baseOrigin(page)).toBe(origin);
});

test("undoes the last turn offline, plays on, and a refresh resumes at the undone position", async ({ page, context, baseURL }) => {
  await blockServer(context, new URL(baseURL!).origin);
  const gameId = await startOfflineGame(page, 4);
  await playUntil(page, gameId, 30);
  // Play on to the person's own turn menu, so the log holds a turn to take back.
  for (let i = 0; i < 400 && (await page.getByTestId("turn-action-bar").count()) === 0; i += 1) {
    await clickOnce(page);
    await page.waitForTimeout(150);
  }
  await expect(page.getByTestId("turn-action-bar")).toBeVisible();
  const answers = await storedAnswers(page, gameId);

  const minimize = page.getByTestId("minimize-choice-button");
  if (await minimize.count()) await minimize.first().dispatchEvent("click");
  await page.getByTestId("event-log-toggle").dispatchEvent("click");
  const button = page.getByTestId("turn-redo-btn");
  await expect(button).toHaveText("Undo my last turn");
  await button.dispatchEvent("click");
  await expect.poll(() => storedAnswers(page, gameId), { timeout: 60_000 }).toBeLessThan(answers);
  const undone = await storedAnswers(page, gameId);
  expect(undone).toBeGreaterThan(0);
  await expect(page.getByTestId("turn-action-bar")).toBeVisible({ timeout: 60_000 });

  // A refresh replays the shorter save to the same place, and the game goes on from there.
  await page.reload();
  await page.waitForSelector('[data-testid="offline-game"]', { timeout: 60_000 });
  expect(await storedAnswers(page, gameId)).toBe(undone);
  await playUntil(page, gameId, undone + 5);
});

test("the map picker: untouched keeps the recommended layout, a pick is stored in the save and survives a resume", async ({
  page,
  context,
  baseURL,
}) => {
  await blockServer(context, new URL(baseURL!).origin);
  // Untouched: the save has no map (old and new games alike take the engine's recommended layout).
  const plain = await startOfflineGame(page, 4);
  expect(await storedSetup(page, plain)).not.toHaveProperty("map");

  const open = async (players: number) => {
    await page.goto("/offline");
    await page.getByLabel("Nickname").fill(NICKNAME);
    await page.getByLabel("Players").selectOption(String(players));
    await page.getByTestId("offline-map-button").click();
    // The engine answers: real layouts for this size and a real picture.
    await expect(page.locator('[data-testid^="map-card-"]:not([data-testid="map-card-random"])').first()).toBeVisible({ timeout: 60_000 });
    await expect(page.getByTestId("map-preview-board")).toBeVisible({ timeout: 30_000 });
  };
  const start = async () => {
    await page.getByTestId("map-picker-done").click();
    await page.getByTestId("offline-start").click();
    await page.waitForSelector('[data-testid="offline-game"]', { timeout: 60_000 });
    return decodeURIComponent(/\/offline\/games\/([^/?#]+)/.exec(page.url())![1]);
  };

  // A listed layout that is not the recommended one.
  await open(4);
  const cards = page.locator('[data-testid^="map-card-"]:not([data-testid="map-card-random"])');
  const ids = await cards.evaluateAll((els) =>
    els.map((el) => ({ id: el.getAttribute("data-testid")!.replace("map-card-", ""), on: el.getAttribute("aria-pressed") === "true" })),
  );
  const other = ids.find((c) => !c.on);
  test.skip(!other, "the engine lists only one layout for four players");
  await page.getByTestId(`map-card-${other!.id}`).click();
  await expect(page.getByTestId(`map-card-${other!.id}`)).toHaveAttribute("aria-pressed", "true");
  const picked = await start();
  expect((await storedSetup(page, picked))?.map).toBe(other!.id);
  await playUntil(page, picked, 4);
  await page.waitForTimeout(400);
  const stored = (await storedRecord(page, picked))!;
  // Resume replays the answers on the same board.
  await page.reload();
  await page.waitForSelector('[data-testid="offline-game"]', { timeout: 120_000 });
  expect(await storedRecord(page, picked)).toMatchObject({ answers: stored.answers, decisionsHash: stored.decisionsHash });
  expect((await storedSetup(page, picked))?.map).toBe(other!.id);

  // Random.
  await open(3);
  await page.getByTestId("map-card-random").click();
  await expect(page.getByTestId("map-card-random")).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByTestId("map-preview-board")).toBeVisible();
  const random = await start();
  expect((await storedSetup(page, random))?.map).toBe("random");
});

test("closing the tab and opening the game later (same IndexedDB) resumes the same position", async ({
  browser,
  baseURL,
}) => {
  const origin = new URL(baseURL!).origin;
  const first = await browser.newContext({ baseURL: baseURL! });
  await blockServer(first, origin);
  const page = await first.newPage();
  const gameId = await startOfflineGame(page, 3);
  await playUntil(page, gameId, 12);
  // Let the last write settle, then read what is stored.
  await page.waitForTimeout(500);
  const stored = (await storedRecord(page, gameId))!;
  expect(stored.answers).toBeGreaterThanOrEqual(12);
  const state = await first.storageState({ indexedDB: true });
  await first.close(); // the tab and the whole browser context are gone

  const second = await browser.newContext({ baseURL: baseURL!, storageState: state });
  await blockServer(second, origin);
  const reopened = await second.newPage();
  // The list of saved games offers it ...
  await reopened.goto("/offline");
  await expect(reopened.getByTestId("offline-game-row")).toHaveCount(1);
  // The list names the faction the engine drew for the human (a random choice is stored too).
  await expect(reopened.getByTestId("offline-game-row")).not.toContainText("Faction pending");
  // ... and resuming replays it to the same position.
  await reopened.getByTestId("offline-resume").click();
  await reopened.waitForSelector('[data-testid="offline-game"]', { timeout: 120_000 });
  expect(reopened.url()).toContain(`/offline/games/${gameId}`);
  await expect(reopened.getByTestId("offline-engine")).toBeVisible();
  const after = (await storedRecord(reopened, gameId))!;
  expect(after).toMatchObject({ answers: stored.answers, decisionsHash: stored.decisionsHash, round: stored.round, phase: stored.phase });
  // And it plays on.
  await playUntil(reopened, gameId, stored.answers + 3);
  await second.close();
});

test("exports a game, imports the file as a second game, and resumes the copy at the same position", async ({
  page,
  context,
  baseURL,
}) => {
  await blockServer(context, new URL(baseURL!).origin);
  const gameId = await startOfflineGame(page, 3);
  await playUntil(page, gameId, 6);
  await page.waitForTimeout(400);
  const saved = (await storedRecord(page, gameId))!;

  await page.goto("/offline");
  const rows = page.getByTestId("offline-game-row");
  await expect(rows).toHaveCount(1);
  // The list names the faction of every seat, and marks the person's.
  await expect(rows.getByTestId("offline-game-factions")).toContainText("(you)");
  await expect(rows.getByTestId("offline-game-factions")).not.toContainText("?");

  const download = page.waitForEvent("download");
  await rows.getByTestId("offline-export").click();
  const file = await (await download).path();
  await page.getByTestId("import-game-file").setInputFiles(file);
  await expect(page.getByTestId("offline-notice")).toHaveText("Game imported.");
  await expect(rows).toHaveCount(2);

  // The copy is the newest game: it opens under its own id, at the position of the file.
  await rows.first().getByTestId("offline-resume").click();
  await page.waitForSelector('[data-testid="offline-game"]', { timeout: 120_000 });
  const copyId = /\/offline\/games\/([^/?#]+)/.exec(page.url())![1];
  expect(copyId).not.toBe(gameId);
  expect(await storedRecord(page, copyId)).toMatchObject({ answers: saved.answers, decisionsHash: saved.decisionsHash });
  // It plays on, and the original stays as it was.
  await playUntil(page, copyId, saved.answers + 2);
  expect(await storedRecord(page, gameId)).toMatchObject({ answers: saved.answers, decisionsHash: saved.decisionsHash });
});

test("a save of an older version is not opened: the list offers to delete it", async ({ page, context, baseURL }) => {
  await blockServer(context, new URL(baseURL!).origin);
  await page.goto("/offline");
  await expect(page.getByTestId("offline-games-empty")).toBeVisible();
  // A record of the format before (record schema 1, save format 1).
  await page.evaluate(
    () =>
      new Promise<void>((done, fail) => {
        const open = indexedDB.open("ti4-offline");
        open.onerror = () => fail(new Error("open"));
        open.onsuccess = () => {
          const db = open.result;
          const tx = db.transaction("games", "readwrite");
          tx.objectStore("games").put({
            schema: 1,
            id: "off_old",
            nickname: "Tester",
            createdAt: 1,
            updatedAt: 1,
            status: "in_progress",
            save: { format: 1, seed: 7, answers: ["o1"] },
          });
          tx.oncomplete = () => {
            db.close();
            done();
          };
          tx.onerror = () => fail(new Error("write"));
        };
      }),
  );
  await page.reload();
  const older = page.getByTestId("offline-older-saves");
  await expect(older).toContainText("1 saved game from an older version");
  await expect(page.getByTestId("offline-game-row")).toHaveCount(0);
  await older.getByTestId("offline-older-delete").click();
  await older.getByTestId("offline-older-delete-confirm").click();
  await expect(older).toHaveCount(0);
  await page.reload();
  await expect(page.getByTestId("offline-games-empty")).toBeVisible();
  await expect(page.getByTestId("offline-older-saves")).toHaveCount(0);
});

test("a game saved with another engine file asks first: view only keeps it untouched, delete removes it", async ({
  page,
  context,
  baseURL,
}) => {
  await blockServer(context, new URL(baseURL!).origin);
  const gameId = await startOfflineGame(page, 3);
  await playUntil(page, gameId, 4);
  await page.waitForTimeout(400);
  const before = (await storedRecord(page, gameId))!;
  // Pretend another build of the engine saved it.
  await page.evaluate(
    (id) =>
      new Promise<void>((done, fail) => {
        const open = indexedDB.open("ti4-offline");
        open.onerror = () => fail(new Error("open"));
        open.onsuccess = () => {
          const db = open.result;
          const store = db.transaction("games", "readwrite").objectStore("games");
          const get = store.get(id);
          get.onsuccess = () => {
            store.put({ ...get.result, engine: { ...get.result.engine, wasmSha256: "0".repeat(64) } });
          };
          setTimeout(() => {
            db.close();
            done();
          }, 300);
        };
      }),
    gameId,
  );
  expect((await storedRecord(page, gameId))!.wasmSha256).toBe("0".repeat(64));

  await page.reload();
  const dialog = page.getByTestId("offline-mismatch");
  await expect(dialog).toBeVisible({ timeout: 60_000 });
  await expect(page.getByTestId("offline-game")).toHaveCount(0); // nothing opens before the choice
  await page.getByTestId("offline-mismatch-view-only").click();
  await page.waitForSelector('[data-testid="offline-game"]', { timeout: 120_000 });
  await expect(page.getByTestId("offline-view-only")).toBeVisible();
  const viewed = (await storedRecord(page, gameId))!;
  expect(viewed).toMatchObject({ answers: before.answers, decisionsHash: before.decisionsHash, wasmSha256: "0".repeat(64) });

  await page.reload();
  await expect(page.getByTestId("offline-mismatch")).toBeVisible({ timeout: 60_000 });
  await page.getByTestId("offline-mismatch-delete").click();
  await page.getByTestId("offline-mismatch-delete-confirm").click();
  await expect(page.getByTestId("offline-games-empty")).toBeVisible({ timeout: 30_000 });
  expect(await storedRecord(page, gameId)).toBeNull();
});

test("undo and history rewind work offline", async ({ page, context, baseURL }) => {
  const origin = new URL(baseURL!).origin;
  await blockServer(context, origin);
  const gameId = await startOfflineGame(page, 3);
  await context.setOffline(true);
  const reached = await playUntil(page, gameId, 14);
  await page.waitForTimeout(400);
  const before = (await storedRecord(page, gameId))!;

  // Rewind: the Undo button of an early decision in the event log.
  // (a decision dialog may cover the corners of the board: click through the DOM)
  await page.getByTestId("event-log-toggle").dispatchEvent("click");
  const undo = page.locator(".event-log__undo");
  // (newest first: the last button belongs to the oldest decision)
  await expect(undo.last()).toBeVisible();
  await undo.last().dispatchEvent("click");
  const confirm = page.getByTestId("undo-confirm-accept");
  if (await confirm.isVisible().catch(() => false)) await confirm.dispatchEvent("click");
  await expect.poll(() => storedAnswers(page, gameId), { timeout: 120_000 }).toBeLessThan(reached);
  const rewound = (await storedRecord(page, gameId))!;
  expect(rewound.decisionsHash).not.toBe(before.decisionsHash);
  await expect(page.getByTestId("history-error")).toHaveCount(0);

  // The undone decisions can be redone ...
  const redo = page.getByRole("button", { name: /Redo action/ });
  await expect(redo).toBeVisible();
  await redo.dispatchEvent("click");
  await expect.poll(() => storedAnswers(page, gameId), { timeout: 120_000 }).toBeGreaterThan(rewound.answers);
  // ... and the game plays on after a rewind as well.
  const now = await storedAnswers(page, gameId);
  await playUntil(page, gameId, now + 3);
  await expect(page.getByTestId("history-error")).toHaveCount(0);
});

test("single player: log toggle in the objectives row, swipe a toast, newest first, undo one decision, reload", async ({ page, context, baseURL }) => {
  await blockServer(context, new URL(baseURL!).origin);
  const gameId = await startOfflineGame(page, 3);
  // Play until a toast is on screen.
  let guard = 0;
  // (a decision dialog's backdrop sits over the toasts: wait for the action phase's bar)
  while (((await page.getByTestId("corner-toast").count()) === 0 || (await page.getByTestId("turn-action-bar").count()) === 0) && guard++ < 120) {
    await clickOnce(page);
    await page.waitForTimeout(200);
  }
  await expect(page.getByTestId("corner-toast").first()).toBeVisible();

  // The notification log is an icon in the row of the objective buttons, and the old pill is gone.
  const toggle = page.getByTestId("toast-history-toggle");
  await expect(toggle).toHaveAttribute("aria-label", /Notifications/);
  await expect(toggle).toHaveText(/^\d*\+?$/);
  const objectives = (await page.getByTestId("objectives-modal-button").boundingBox())!;
  const bell = (await toggle.boundingBox())!;
  expect(Math.abs(bell.y - objectives.y)).toBeLessThan(6);
  await expect(page.getByText(/Recent notifications \(/)).toHaveCount(0);

  // Swipe (mouse drag) a toast away: gone from the screen, still in the log.
  const toasts = page.getByTestId("corner-toast");
  const count = await toasts.count();
  const first = toasts.first();
  const phrase = ((await first.locator(".toast-value").textContent()) ?? "").trim().slice(0, 24);
  const box = (await first.boundingBox())!;
  const y = box.y + box.height / 2;
  await page.mouse.move(box.x + box.width / 2, y);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 - 140, y, { steps: 5 });
  await page.mouse.up();
  await expect.poll(() => toasts.count(), { timeout: 3000 }).toBeLessThan(count);
  await toggle.click();
  await expect(page.getByTestId("toast-history-item").filter({ hasText: phrase }).first()).toBeVisible();
  await toggle.click();

  // Play on, then the event log lists the newest entry first.
  const reached = await playUntil(page, gameId, 8);
  await page.waitForTimeout(500);
  await page.getByTestId("event-log-toggle").dispatchEvent("click");
  const numbers = await page.getByTestId("event-log-entry").evaluateAll((rows) =>
    rows.map((r) => Number(/#(\d+)/.exec(r.querySelector(".event-log__index")?.textContent ?? "")?.[1])).filter((n) => !Number.isNaN(n)),
  );
  expect(numbers.length).toBeGreaterThan(3);
  expect(numbers[0]).toBeGreaterThan(numbers[numbers.length - 1]);

  // Undo takes back exactly one answer of the human; the save follows; a reload resumes there.
  const answers = await storedAnswers(page, gameId);
  expect(answers).toBeGreaterThanOrEqual(reached);
  await expect(page.getByTestId("event-log-undo-last")).toBeEnabled();
  await page.getByTestId("event-log-undo-last").dispatchEvent("click");
  await expect.poll(() => storedAnswers(page, gameId), { timeout: 60_000 }).toBe(answers - 1);
  await expect(page.getByTestId("event-log-undo-status")).toContainText("Undid your last decision");
  await page.reload();
  await page.waitForSelector('[data-testid="offline-game"]', { timeout: 60_000 });
  expect(await storedAnswers(page, gameId)).toBe(answers - 1);
  await expect(page.getByTestId("history-error")).toHaveCount(0);
});

test("single player: the browser Back button undoes the last decision; with nothing to undo it leaves the game", async ({ page, context, baseURL }) => {
  await blockServer(context, new URL(baseURL!).origin);
  const gameId = await startOfflineGame(page, 3);
  const reached = await playUntil(page, gameId, 6);
  await page.waitForTimeout(600);
  const url = page.url();
  await page.goBack();
  await expect.poll(() => storedAnswers(page, gameId), { timeout: 60_000 }).toBe(reached - 1);
  expect(page.url()).toBe(url); // stayed in the game
  await expect(page.getByTestId("back-undo-note")).toContainText("Undid your last decision");
  // A reload keeps one sentinel only: Back still undoes one decision, not two.
  await page.reload();
  await page.waitForSelector('[data-testid="offline-game"]', { timeout: 60_000 });
  expect(await storedAnswers(page, gameId)).toBe(reached - 1);
  await page.waitForTimeout(600);
  await page.goBack();
  await expect.poll(() => storedAnswers(page, gameId), { timeout: 60_000 }).toBe(reached - 2);
  expect(page.url()).toBe(url);
  // Forward does nothing harmful.
  await page.goForward().catch(() => undefined);
  expect(await storedAnswers(page, gameId)).toBe(reached - 2);
});

test("single player: Back at the start of a game, with nothing to undo, goes back to the offline start page", async ({ page, context, baseURL }) => {
  await blockServer(context, new URL(baseURL!).origin);
  const gameId = await startOfflineGame(page, 3);
  await page.waitForTimeout(500);
  await page.goBack();
  await expect.poll(() => new URL(page.url()).pathname, { timeout: 10_000 }).toBe("/offline");
  expect(await storedAnswers(page, gameId)).toBe(0);
});

/** Every report that the server stored (parsed JSON), the `.ids` markers of its idempotency check left out. */
function storedReports(server: TestServer): Array<Record<string, unknown>> {
  const walk = (dir: string): string[] =>
    readdirSync(dir).flatMap((name) => {
      if (name.startsWith(".")) return [];
      const path = join(dir, name);
      return statSync(path).isDirectory() ? walk(path) : path.endsWith(".json") ? [path] : [];
    });
  const root = join(server.dataDir, "feedback");
  return existsSync(root) ? walk(root).map((file) => JSON.parse(readFileSync(file, "utf8"))) : [];
}

// A real ti4-server (TI4_E2E_SERVER_BIN, else the debug or release binary of the target dir), so
// that a queued feedback report is proven to reach its feedback directory exactly once.
test.describe("feedback written offline", () => {
  let server: TestServer;
  test.beforeAll(async () => {
    server = await spawnTestServer();
  });
  test.afterAll(async () => {
    await server?.stop();
  });

  test("is delivered once the network returns, and exactly once when the answer is lost", async ({
    page,
    context,
    baseURL,
    request,
  }) => {
    const origin = new URL(baseURL!).origin;
    await blockServer(context, origin);
    // /api/feedback is the one call that is let through to the test server, and only when "online".
    let serverUp = false;
    let dropNextResponse = false;
    let forwarded = 0;
    await context.route("**/api/feedback", async (route) => {
      if (!serverUp) return route.abort("internetdisconnected");
      const reply = await request.post(`${server.url}/api/feedback`, {
        data: route.request().postDataJSON(),
      });
      forwarded += 1;
      if (dropNextResponse) {
        dropNextResponse = false;
        return route.abort("connectionreset");
      }
      return route.fulfill({ response: reply });
    });

    const gameId = await startOfflineGame(page, 3);
    await context.setOffline(true);
    await playUntil(page, gameId, 6);
    await page.waitForTimeout(300);

    // Send feedback while offline: it is queued, the game goes on.
    await page.getByTestId("feedback-button").dispatchEvent("click");
    await page.getByTestId("feedback-text").fill("The offline report: something looked odd in round 1.");
    await page.getByTestId("feedback-submit").click();
    await expect(page.getByTestId("feedback-queued")).toBeVisible();
    await page.getByTestId("feedback-done").dispatchEvent("click");
    await expect(page.getByTestId("offline-feedback-chip")).toContainText("will be sent");
    expect(storedReports(server)).toHaveLength(0);
    await playUntil(page, gameId, 8); // never blocked by the queue

    // The network returns, but the answer of the first send is lost on the way back.
    serverUp = true;
    dropNextResponse = true;
    await context.setOffline(false);
    await expect.poll(() => forwarded, { timeout: 30_000 }).toBe(1);
    expect(storedReports(server)).toHaveLength(1);
    await expect(page.getByTestId("offline-feedback-chip")).toContainText("will be sent");

    // A second attempt (the browser says it is online again): the server answers duplicate.
    await context.setOffline(true);
    await context.setOffline(false);
    await expect.poll(async () => (await storedFeedback(page)).map((i) => i.state), { timeout: 30_000 }).toEqual(["sent"]);
    expect(forwarded).toBe(2);
    await expect(page.getByTestId("offline-feedback-chip")).toContainText("sent");

    // Exactly one file, with what a triage needs to replay the game.
    const reports = storedReports(server);
    expect(reports).toHaveLength(1);
    const report = reports[0] as { report_id?: string; context: { local?: Record<string, any> } };
    const [queued] = await storedFeedback(page);
    expect(report.report_id).toBe(queued.reportId);
    const local = report.context.local!;
    expect(local.wasm_sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(local.engine.build).toBeTruthy();
    expect(local.save.answers.length).toBeGreaterThanOrEqual(6); // the position when the report was written
    expect(local.answers).toEqual(local.save.answers);
  });
});
