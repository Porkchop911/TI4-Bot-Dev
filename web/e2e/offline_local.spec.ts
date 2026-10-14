import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { playUntil, startOfflineGame, storedAnswers, storedFeedback, storedRecord } from "./offlineDriver";
import { startTestServer, type TestServer } from "./offlineServer";

// The offline game, end to end, with the production build only (see offline.config.ts): the
// engine runs in a Web Worker (JSPI), and nothing but the built static files is ever fetched.

const baseOrigin = (page: Page) => new URL(page.url() || "http://127.0.0.1").origin;

/** Only the built files are served; every call to a server (and anything off-origin) fails. */
async function blockServer(context: BrowserContext, origin: string, requests: string[] = []) {
  await context.route("**/*", (route) => {
    const url = new URL(route.request().url());
    if (url.origin !== origin || /^\/(api|ws|advisor|health)(\/|$)/.test(url.pathname)) return route.abort();
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
  // Hidden offline: turn redo, advisor.
  await expect(page.getByTestId("turn-redo-btn")).toHaveCount(0);
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
  await expect(undo.first()).toBeVisible();
  await undo.first().dispatchEvent("click");
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

test.describe("feedback written offline", () => {
  let server: TestServer;
  test.beforeAll(async () => {
    server = await startTestServer();
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
    expect(server.reports()).toHaveLength(0);
    await playUntil(page, gameId, 8); // never blocked by the queue

    // The network returns, but the answer of the first send is lost on the way back.
    serverUp = true;
    dropNextResponse = true;
    await context.setOffline(false);
    await expect.poll(() => forwarded, { timeout: 30_000 }).toBe(1);
    expect(server.reports()).toHaveLength(1);
    await expect(page.getByTestId("offline-feedback-chip")).toContainText("will be sent");

    // A second attempt (the browser says it is online again): the server answers duplicate.
    await context.setOffline(true);
    await context.setOffline(false);
    await expect.poll(async () => (await storedFeedback(page)).map((i) => i.state), { timeout: 30_000 }).toEqual(["sent"]);
    expect(forwarded).toBe(2);
    await expect(page.getByTestId("offline-feedback-chip")).toContainText("sent");

    // Exactly one file, with what a triage needs to replay the game.
    const reports = server.reports();
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
