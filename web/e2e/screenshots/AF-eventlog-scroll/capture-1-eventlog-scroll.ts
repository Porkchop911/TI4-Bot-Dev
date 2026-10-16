/**
 * Regression spec for audit item L7: the event log follows new events while the reader is at the
 * bottom, leaves a reader who scrolled up alone, keeps manual expansion across a reconnect and keeps
 * its position across hide/show (docked log and phone drawer).
 *
 * Runs against the mocked game (no backend):
 *   cd web && npm run screenshots -- AF
 * Measurements go to out/results.json (and are printed in the test names' attachments); screenshots
 * are out/*.png and feed the generated index.html.
 *
 * Measured: scrollTop / scrollHeight / clientHeight of [data-testid=event-log-list] before and after
 * each new log entry (the mock pushes `{type:"event"}` messages, or a new `initial_snapshot` for a
 * reconnect / history-generation change). The "before the fix" numbers are in manifest.json.
 */
import { expect, test, type Page, type TestInfo } from "@playwright/test";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { PROTOCOL_VERSION, type GameEvent } from "../../../src/protocol/types";
import { GAME_ID, openMockedGame, type MockedGame } from "../_shared/mockGame";
import { actor } from "../_shared/fixtures";
import { shot } from "../_shared/shot";

const OUT = join(dirname(new URL(import.meta.url).pathname), "out");
const FILE = join(OUT, "results.json");
/** Merges one scenario's measurements into out/results.json. */
const results = new Proxy({} as Record<string, unknown>, {
  set(_t, key: string, value) {
    mkdirSync(OUT, { recursive: true });
    const all = existsSync(FILE) ? JSON.parse(readFileSync(FILE, "utf8")) : {};
    all[key] = value;
    writeFileSync(FILE, JSON.stringify(all, null, 2));
    return true;
  },
});

const decision = (n: number, round: number, phase: string, extra: Partial<GameEvent> = {}): GameEvent =>
  ({
    id: `g-${n}`,
    timestamp: "10:05",
    visibility: { visibility: "public" as const },
    event: { kind: "decision_resolved" },
    round,
    phase,
    actor,
    decision_count: n,
    detail: `${actor} did thing number ${n}`,
    ...extra,
  }) as GameEvent;
const marker = (id: string, round: number, phase: string): GameEvent =>
  ({ id, visibility: { visibility: "public" as const }, event: { kind: "phase_transition", round, phase } }) as GameEvent;

/** Round 1: 40 decisions in the action phase; round 2: 30 decisions. */
function history(): GameEvent[] {
  const out: GameEvent[] = [
    { id: "init", visibility: { visibility: "public" as const }, event: { kind: "game_initialized", round: 1, phase: "action", speaker: actor } } as GameEvent,
  ];
  let n = 0;
  for (let i = 0; i < 40; i++) out.push(decision(++n, 1, "action"));
  out.push(marker("pt-2", 2, "action"));
  for (let i = 0; i < 30; i++) out.push(decision(++n, 2, "action"));
  return out;
}
let counter = 1000;
const next = (round = 2, phase = "action") => decision(++counter, round, phase);
const push = (game: MockedGame, entry: GameEvent) =>
  game.send({ type: "event", protocol_version: PROTOCOL_VERSION, game_id: GAME_ID, entry });
/** history.cursor must already cover the log: otherwise the first live event flips the cursor and
 *  every old row gains an Undo button (a mock artifact: +2px per row), which would pollute the numbers. */
const hist = (generation: number) => ({ cursor: 100, redo_count: 0, generation });

const list = (page: Page) => page.getByTestId("event-log-list");
const metrics = (page: Page) =>
  list(page).evaluate((el) => ({
    scrollTop: Math.round(el.scrollTop),
    scrollHeight: el.scrollHeight,
    clientHeight: el.clientHeight,
    bottomGap: Math.round(el.scrollHeight - el.clientHeight - el.scrollTop),
    entries: el.querySelectorAll('[data-testid="event-log-entry"]').length,
    // Heading text without the entry-count badge (it changes with every event).
    openHeadings: [...el.querySelectorAll('button[aria-expanded="true"]')].map((b) => [...b.childNodes].filter((n) => n.nodeType === 3).map((n) => n.textContent).join("").trim()),
    // What is on top at the middle of the list's bottom edge (the turn action bar covers it on a phone).
    hitAtBottom: (() => {
      const r = el.getBoundingClientRect();
      const hit = document.elementFromPoint(r.left + r.width / 2, r.bottom - 4);
      return hit ? `${hit.tagName.toLowerCase()}.${String(hit.className).split(" ")[0]}${el.contains(hit) ? "" : " (NOT in list)"}` : "none";
    })(),
  }));
const settle = (page: Page) => page.waitForTimeout(250);
const isPhone = (vp: { width: number }) => vp.width <= 900;
const picture = (page: Page, testInfo: TestInfo, name: string) =>
  shot(page, testInfo, name, { of: page.locator("#event-log-drawer"), pad: 8 });
/** The docked toggle on a desktop, the "Events" button on a phone: shows or hides the log. */
const toggleLog = (page: Page, vp: { width: number }) =>
  isPhone(vp) ? page.getByRole("button", { name: "Events", exact: true }).click() : page.getByTestId("event-log-toggle").click();

async function open(page: Page, vp: { width: number; height: number }, events = history()) {
  await page.setViewportSize(vp);
  const game = await openMockedGame(page, { events, version: 40 });
  await page.getByTestId("ti4-board-svg").waitFor();
  // Server tells the client where the game currently is: opens Round 2 > Action phase.
  const { events: _e, ...update } = game.snapshot;
  game.send({ ...update, type: "state_update", current_path: { round: 2, phase: "action" }, history: hist(1) });
  await toggleLog(page, vp);
  await list(page).waitFor();
  await settle(page);
  return game;
}

const DESKTOP = { width: 1440, height: 900 };
const PHONE = { width: 390, height: 844 };
const TOLERANCE = 2; // px; the pin threshold in the component is 8

for (const [vpName, vp] of [["desktop", DESKTOP], ["phone", PHONE]] as const) {
  test.describe(vpName, () => {
    test(`${vpName} (a) pinned to bottom, new event`, async ({ page }, testInfo) => {
      const game = await open(page, vp);
      const initial = await metrics(page); // a freshly opened log starts at the newest entry
      const before = await metrics(page);
      push(game, next());
      await settle(page);
      const after = await metrics(page);
      results[`${vpName}-a`] = { initial, before, after };
      await picture(page, testInfo, `${vpName}-a-pinned`);
      expect(initial.bottomGap, "opens at the newest entry").toBeLessThanOrEqual(TOLERANCE);
      expect(after.bottomGap, "log stays pinned to the bottom").toBeLessThanOrEqual(TOLERANCE);
      expect(after.entries).toBe(before.entries + 1);
    });

    test(`${vpName} (b) scrolled to top/middle, new event (guard)`, async ({ page }, testInfo) => {
      const game = await open(page, vp);
      const r: Record<string, unknown> = {};
      let unseen = 0;
      for (const [label, frac] of [["top", 0], ["middle", 0.5]] as const) {
        await list(page).evaluate((el, f) => (el.scrollTop = (el.scrollHeight - el.clientHeight) * f), frac);
        await settle(page);
        const before = await metrics(page);
        push(game, next());
        unseen++;
        await settle(page);
        const after = await metrics(page);
        r[label] = { before, after };
        expect(after.scrollTop, `${label}: position preserved`).toBe(before.scrollTop);
        await expect(page.getByTestId("event-log-jump"), `${label}: jump row counts what arrived`).toContainText(`${unseen} new event`);
        if (label === "top") await picture(page, testInfo, `${vpName}-b-jump`);
      }
      // The jump row takes the reader to the newest entry and goes away.
      await page.getByTestId("event-log-jump").click();
      await settle(page);
      const jumped = await metrics(page);
      r.jumped = jumped;
      results[`${vpName}-b`] = r;
      expect(jumped.bottomGap).toBeLessThanOrEqual(TOLERANCE);
      await expect(page.getByTestId("event-log-jump")).toHaveCount(0);
    });

    test(`${vpName} (c) expanded older node while event arrives (guard)`, async ({ page }) => {
      const game = await open(page, vp);
      // Open Round 1 (manual) and its phase, then look at where we are.
      await list(page).getByRole("button", { name: /Round 1/ }).dispatchEvent("click"); // phone: the turn bar overlaps the drawer, so no pointer click
      const phase1 = list(page).getByRole("button", { name: /Action phase/ }).first();
      if ((await phase1.getAttribute("aria-expanded")) === "false") await phase1.dispatchEvent("click");
      await settle(page);
      await list(page).evaluate((el) => (el.scrollTop = 300));
      const before = await metrics(page);
      push(game, next());
      await settle(page);
      const after = await metrics(page);
      // Then a state_update with the same current_path (the server sends one after every decision).
      const { events: _e, ...update } = game.snapshot;
      game.send({ ...update, type: "state_update", game_version: 41, current_path: { round: 2, phase: "action" } });
      await settle(page);
      const afterUpdate = await metrics(page);
      results[`${vpName}-c`] = { before, after, afterStateUpdate: afterUpdate };
      expect(after.openHeadings, "expanded nodes stay open").toEqual(before.openHeadings);
      expect(after.scrollTop, "no jump").toBe(before.scrollTop);
      expect(afterUpdate.openHeadings).toEqual(before.openHeadings);
    });

    test(`${vpName} (d) burst of events`, async ({ page }) => {
      const game = await open(page, vp);
      const r: Record<string, unknown> = {};
      for (const [label, pinned] of [["pinned", true], ["scrolled-up", false]] as const) {
        await list(page).evaluate((el, p) => (el.scrollTop = p ? el.scrollHeight : 120), pinned);
        await settle(page);
        const before = await metrics(page);
        for (let i = 0; i < 8; i++) push(game, next());
        await settle(page);
        const after = await metrics(page);
        r[label] = { before, after };
        if (pinned) expect(after.bottomGap, "burst: pinned").toBeLessThanOrEqual(TOLERANCE);
        else expect(after.scrollTop, "burst: position preserved").toBe(before.scrollTop);
      }
      results[`${vpName}-d`] = r;
    });

    test(`${vpName} (e) new round / phase node`, async ({ page }, testInfo) => {
      const game = await open(page, vp);
      const before = await metrics(page);
      push(game, marker("pt-3", 3, "strategy"));
      push(game, decision(++counter, 3, "strategy"));
      await settle(page);
      const afterNoPath = await metrics(page);
      // The server then moves current_path to the new round, which auto-opens it.
      const { events: _e, ...update } = game.snapshot;
      game.send({ ...update, type: "state_update", game_version: 42, current_path: { round: 3, phase: "strategy" } });
      await settle(page);
      const afterPath = await metrics(page);
      results[`${vpName}-e`] = { before, afterNoPath, afterPath };
      await picture(page, testInfo, `${vpName}-e-new-round`);
      expect(afterNoPath.bottomGap, "pinned while the new node is still closed").toBeLessThanOrEqual(TOLERANCE);
      expect(afterPath.bottomGap, "pinned after new round opens").toBeLessThanOrEqual(TOLERANCE);
      expect(afterPath.openHeadings.join("|")).toContain("Round 3");
    });

    test(`${vpName} (f) reconnect keeps expansion and scroll; a new generation resets sensibly`, async ({ page }, testInfo) => {
      const game = await open(page, vp);
      await list(page).getByRole("button", { name: /Round 1/ }).dispatchEvent("click"); // phone: the turn bar overlaps the drawer, so no pointer click
      await settle(page);
      await list(page).evaluate((el) => (el.scrollTop = 200));
      const before = await metrics(page);
      // Reconnect: the server sends a fresh initial_snapshot (new events array), same generation.
      game.send({ ...game.snapshot, game_version: 50, events: [...history(), decision(9001, 2, "action")], current_path: { round: 2, phase: "action" }, history: hist(1) });
      await settle(page);
      const afterReconnect = await metrics(page);
      await picture(page, testInfo, `${vpName}-f-reconnect`);
      expect(afterReconnect.openHeadings, "manually opened Round 1 survives a reconnect").toEqual(before.openHeadings);
      expect(afterReconnect.scrollTop, "scroll position survives a reconnect").toBe(before.scrollTop);
      // Undo: a new history generation with a shorter log. The reset is allowed, the list stays valid.
      game.send({ ...game.snapshot, game_version: 51, events: history().slice(0, -5), current_path: { round: 2, phase: "action" }, history: hist(2) });
      await settle(page);
      const afterUndo = await metrics(page);
      results[`${vpName}-f`] = { before, afterReconnect, afterUndo };
      expect(afterUndo.scrollTop, "scrollTop is clamped into the shorter list").toBeLessThanOrEqual(afterUndo.scrollHeight - afterUndo.clientHeight);
      expect(afterUndo.openHeadings.join("|"), "a new generation resets manual expansion").not.toContain("Round 1");
    });

    test(`${vpName} (f2) a pinned reader stays pinned across a new generation`, async ({ page }) => {
      const game = await open(page, vp);
      const before = await metrics(page);
      game.send({ ...game.snapshot, game_version: 60, events: [...history().slice(0, -10), decision(9100, 2, "action")], current_path: { round: 2, phase: "action" }, history: hist(2) });
      await settle(page);
      const after = await metrics(page);
      results[`${vpName}-f2`] = { before, after };
      expect(before.bottomGap).toBeLessThanOrEqual(TOLERANCE);
      expect(after.bottomGap, "still at the newest entry after undo").toBeLessThanOrEqual(TOLERANCE);
    });

    // (h) hide/show: the toggle (docked) or the Events button (phone drawer) unmounts the list.
    test(`${vpName} (h) hidden then reopened log, events while hidden`, async ({ page }, testInfo) => {
      const game = await open(page, vp);
      const r: Record<string, unknown> = {};
      // Scrolled up: the position is restored.
      await list(page).evaluate((el) => (el.scrollTop = 150));
      await settle(page);
      const before = await metrics(page);
      await toggleLog(page, vp);
      await settle(page);
      push(game, next());
      push(game, next());
      await settle(page);
      await toggleLog(page, vp);
      await list(page).waitFor();
      await settle(page);
      const reopened = await metrics(page);
      r.scrolledUp = { before, reopened };
      await picture(page, testInfo, `${vpName}-h-reopened`);
      expect(reopened.scrollTop, "scroll position survives hide/show").toBe(before.scrollTop);
      // Pinned: reopening lands on the newest entry, including what arrived while hidden.
      await list(page).evaluate((el) => (el.scrollTop = el.scrollHeight));
      await settle(page);
      await toggleLog(page, vp);
      push(game, next());
      await settle(page);
      await toggleLog(page, vp);
      await list(page).waitFor();
      await settle(page);
      const reopenedPinned = await metrics(page);
      r.pinned = { reopened: reopenedPinned };
      results[`${vpName}-h`] = r;
      expect(reopenedPinned.bottomGap).toBeLessThanOrEqual(TOLERANCE);
    });
  });
}

// Layout shift: another player's event makes a toast; it must not move or resize the log box.
test("desktop (toast) other player's event does not move the log box (guard)", async ({ page }) => {
  const game = await open(page, DESKTOP);
  const box = () => page.getByTestId("event-log-container").boundingBox();
  const before = await box();
  push(game, { ...next(), actor: "other_seat", detail: "other_seat played Sabotage" } as GameEvent);
  await page.waitForTimeout(400);
  const after = await box();
  results["desktop-toast"] = { before, after };
  expect(after?.y).toBe(before?.y);
  expect(after?.height).toBe(before?.height);
});
