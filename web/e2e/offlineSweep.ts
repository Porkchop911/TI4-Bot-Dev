import { appendFileSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { BrowserContext, Page } from "@playwright/test";
import type { BoardView } from "../src/protocol/types";
import {
  DECISION_CONTAINERS,
  ERROR_BANNERS,
  activationWeights,
  collectCandidates,
  escapePaymentDeadEnd,
  mulberry32,
  pick,
  visibleErrors,
} from "./smokePlaythrough";
import { isReactionSubtype, OVERLAY_CLOSE_IDS, reactionTextProblem } from "./smokePolicy";
import { storedRecord } from "./offlineDriver";
import { preferTechWindowUse, settledGameOverBanner } from "./smokePolicy";

/**
 * One full offline game (setup form to game over) driven through the real UI, for the nightly
 * offline sweep (tools/nightly-smoke/offline-run.sh). The page is the production build served as
 * static files; the engine runs as wasm in the page's worker and the other seats are rule bots,
 * so there is no server and no proctor. Every decision of the human seat is answered by clicking
 * the controls the UI offers (the smoke harness's candidate scan and weighted pick), and the
 * worker's messages are observed from the page, so the harness knows the engine's question, its
 * save blob and its end without reading the DOM.
 *
 * Nothing here throws on a UI problem: each one becomes a finding (with a screenshot) and the run
 * ends with an outcome (finished, decision-cap, time-cap, stuck, crash). The Playwright test only
 * fails when the harness itself breaks.
 */

export interface OfflineSweepOptions {
  baseUrl: string;
  gameSeed: number;
  clickSeed: number;
  players: number;
  policy: "random" | "steer";
  /** `random`, or a 1-based seat number. */
  seat: string;
  /** `random` (a random roster faction), `dealt` (the seed deals it) or a faction id. */
  faction: string;
  maxDecisions: number;
  maxSeconds: number;
  /** No worker message and no decision for this long means the game is stuck. */
  stallSeconds: number;
  viewport: "desktop" | "phone";
  /** Reload the page once at this decision and check that the game comes back (null: never). */
  resumeAt: number | null;
  shotCap: number;
  traceDir: string;
  log: (line: string) => void;
}

export type Outcome = "finished" | "decision-cap" | "time-cap" | "stuck" | "crash" | "setup-failed";

export interface OfflineSweepReport {
  outcome: Outcome;
  gameSeed: number;
  clickSeed: number;
  players: number;
  policy: string;
  viewport: string;
  humanSeat: number | null;
  humanFaction: string | null;
  factions: string[];
  decisions: number;
  clicks: number;
  finished: boolean;
  roundStarts: Record<number, number>;
  subtypes: Record<string, number>;
  rejections: string[];
  findings: string[];
  panics: string[];
  consoleErrors: string[];
  pageErrors: string[];
  failedRequests: string[];
  recoveries: number;
  /** Times the harness closed the system inspector because it covered the controls of the question. */
  overlayDismissals: number;
  /** Committing clicks that did not get a new question within the wait (the harness scanned again). */
  commitTimeouts: number;
  /** The resume check, when the run had one. */
  resume?: { at: number; answers: number; ms: number; ok: boolean };
  commitTimeoutControls: Record<string, number>;
  seconds: number;
  engine: { build?: string; content_digest?: string; wasmSha256?: string } | null;
  finalDecisionsHash: string | null;
  finalStandings: string[];
  shots: { taken: number; cap: number; names: string[] };
  finalStatus: unknown;
  /** Wall time the human waited for the engine/bots to ask again, summed (ms) and the worst single wait. */
  engineWaitMs: number;
  worstWaitMs: number;
  heapMb: number | null;
  /** Wall time of the harness by phase, ms: scan (candidate scan), click, commit (wait for the engine to ask again), idle (nothing to do, the bots play). */
  timing: Record<string, number>;
  decisionsPerMinute: number;
}

interface AskSummary {
  seq: number;
  version: number;
  answers: number;
  round: number | null;
  phase: string | null;
  statusKind: string | null;
  nonce: string | null;
  player: string | null;
  subtype: string;
  prompt: string;
  source: unknown;
  options: { id: string; kind?: string; label?: string }[];
  board?: BoardView;
}

/** Runs in the page before any script: seeds the launcher and spies on the engine's worker. */
export function sweepInitScript(arg: { seed: number; autoLone: boolean | null }) {
  const hi = Math.floor(arg.seed / 2 ** 32);
  const lo = arg.seed % 2 ** 32;
  const real = crypto.getRandomValues.bind(crypto);
  // The launcher draws its game seed with a two-word Uint32Array; nothing else does.
  (crypto as unknown as { getRandomValues: unknown }).getRandomValues = (array: ArrayBufferView) => {
    if (array instanceof Uint32Array && array.length === 2) {
      array[0] = hi;
      array[1] = lo;
      return array;
    }
    return real(array as Uint32Array);
  };
  if (arg.autoLone !== null) {
    try {
      localStorage.setItem("player_auto_submit_lone", String(arg.autoLone));
    } catch {
      // storage unavailable
    }
  }
  const w = window as unknown as Record<string, any>;
  const Original = window.Worker;
  let seq = 0;
  const post = (name: string, payload: unknown) => {
    try {
      w.__sweepEvent(name, payload);
    } catch {
      // the binding is not there yet
    }
  };
  w.Worker = function (...args: unknown[]) {
    const worker = new (Original as any)(...args);
    worker.addEventListener("message", (event: MessageEvent) => {
      const m = event.data;
      if (!m || typeof m.t !== "string") return;
      const stamp = Date.now();
      if (m.t === "ask" || m.t === "end") {
        const messages: any[] = m.envelope?.messages ?? [];
        const update = [...messages].reverse().find((x) => x && x.type === "state_update");
        const initial = messages.find((x) => x && x.type === "initial_snapshot");
        const view = update ?? initial ?? null;
        const choice = view?.pending_choice?.choice ?? null;
        const save = m.save ?? m.envelope?.save ?? null;
        w.__sweepSave = save ?? w.__sweepSave;
        const summary: any = {
          kind: m.t,
          stamp,
          seq: m.t === "ask" ? ++seq : seq,
          version: view?.game_version ?? save?.version ?? -1,
          answers: save?.answers?.length ?? -1,
          round: view?.turn_status?.round ?? save?.round ?? null,
          phase: view?.turn_status?.phase ?? save?.phase ?? null,
          statusKind: view?.turn_status?.kind ?? null,
          nonce: view?.pending_choice?.nonce ?? null,
          player: choice?.player ?? null,
          subtype: choice?.context?.subtype ?? (choice ? `prompt:${String(choice.prompt).slice(0, 40)}` : "none"),
          prompt: choice?.prompt ?? "",
          source: choice?.context?.source ?? null,
          options: (choice?.options ?? []).map((o: any) => ({ id: o.id, kind: o.kind, label: o.label })),
          end: m.t === "end" ? (m.envelope?.end ?? null) : undefined,
        };
        summary.events = messages
          .filter((x) => x && x.type === "event" && x.entry)
          .slice(0, 200)
          .map((x) => ({ v: x.entry.version, round: x.entry.round, phase: x.entry.phase, actor: x.entry.actor, detail: String(x.entry.detail ?? x.entry.event?.kind ?? "").slice(0, 200) }));
        if (choice?.context?.subtype === "activate_system") summary.board = view?.view?.board;
        if (view) {
          summary.players = (view.view?.players ?? view.initial?.players ?? []).map((p: any) => ({
            id: p.id,
            faction: p.faction,
            vp: p.victory_points,
          }));
          summary.viewer = view.viewer ?? null;
        }
        post("ask", summary);
      } else if (m.t === "crash" || m.t === "fatal") {
        post(m.t, { stamp, message: String(m.message).slice(0, 2000) });
      } else if (m.t === "progress") {
        post("progress", { stamp });
      }
    });
    return worker;
  } as unknown as typeof Worker;
  w.Worker.prototype = Original.prototype;
}

// The page probes /api/access when it loads; there is no server in the offline sweep (vite preview
// answers 502), so any call to the server's paths is environment noise, counted but not reported.
const SERVER_PATH = /^https?:\/\/[^/]+\/(api|ws|advisor|health)(\/|$|\?)/;

const COMMIT_WAIT_MS = Number(process.env.TI4_OFFLINE_COMMIT_MS ?? 1200);

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export async function playOfflineGame(
  context: BrowserContext,
  page: Page,
  options: OfflineSweepOptions,
): Promise<OfflineSweepReport> {
  const log = options.log;
  const started = Date.now();
  const rng = mulberry32(options.clickSeed);
  mkdirSync(join(options.traceDir, "shots"), { recursive: true });
  const report: OfflineSweepReport = {
    outcome: "setup-failed",
    gameSeed: options.gameSeed,
    clickSeed: options.clickSeed,
    players: options.players,
    policy: options.policy,
    viewport: options.viewport,
    humanSeat: null,
    humanFaction: null,
    factions: [],
    decisions: 0,
    clicks: 0,
    finished: false,
    roundStarts: {},
    subtypes: {},
    rejections: [],
    findings: [],
    panics: [],
    consoleErrors: [],
    pageErrors: [],
    failedRequests: [],
    recoveries: 0,
    overlayDismissals: 0,
    commitTimeouts: 0,
    commitTimeoutControls: {},
    seconds: 0,
    engine: null,
    finalDecisionsHash: null,
    finalStandings: [],
    shots: { taken: 0, cap: options.shotCap, names: [] },
    finalStatus: null,
    engineWaitMs: 0,
    worstWaitMs: 0,
    heapMb: null,
    timing: { scan: 0, click: 0, commit: 0, idle: 0 },
    decisionsPerMinute: 0,
  };

  let currentGameId = "";
  // --- observers -------------------------------------------------------------------------
  let ask: AskSummary | null = null;
  let lastMessageAt = Date.now();
  let ended: { stamp: number; end: unknown; players?: { id: string; faction: string; vp: number }[] } | null = null;
  let crashed: string | null = null;
  let players: { id: string; faction: string; vp: number }[] = [];
  await page.exposeBinding("__sweepEvent", (_source, name: string, payload: any) => {
    lastMessageAt = Date.now();
    if (payload?.players?.length) players = payload.players;
    if (name === "ask") {
      if (payload.kind === "end") ended = { stamp: payload.stamp, end: payload.end, players: payload.players };
      else if (payload.statusKind === "game_over" && !payload.subtype.startsWith("prompt:") && payload.subtype === "none") {
        // a final state without a question: the end message follows
      }
      for (const event of payload.events ?? []) appendFileSync(join(options.traceDir, "events.jsonl"), `${JSON.stringify(event)}\n`);
      if (payload.kind === "ask") ask = payload as AskSummary;
    } else if (name === "crash" || name === "fatal") {
      crashed = `${name}: ${payload.message}`;
      report.panics.push(crashed);
    }
  });
  await context.addInitScript(sweepInitScript, {
    seed: options.gameSeed,
    autoLone: process.env.TI4_OFFLINE_AUTO_LONE === "0" ? false : process.env.TI4_OFFLINE_AUTO_LONE === "1" ? true : null,
  });
  page.on("pageerror", (error) => {
    const text = `${error.message}`.split("\n")[0].slice(0, 400);
    report.pageErrors.push(text);
    if (/unreachable|RuntimeError|out of memory|memory access/i.test(error.message)) report.panics.push(`pageerror: ${text}`);
    log(`  PAGE ERROR: ${text}`);
  });
  page.on("console", (msg) => {
    if (msg.type() !== "error") return;
    const url = msg.location().url ?? "";
    if (url.includes("favicon") || SERVER_PATH.test(url)) return;
    const text = `${msg.text()}${url ? ` (${url})` : ""}`.slice(0, 500);
    report.consoleErrors.push(text);
    log(`  CONSOLE ERROR: ${text}`);
  });
  page.on("requestfailed", (request) => {
    const url = request.url();
    if (url.includes("favicon") || SERVER_PATH.test(url)) return;
    const line = `${request.method()} ${url} ${request.failure()?.errorText ?? ""}`.slice(0, 300);
    report.failedRequests.push(line);
  });
  page.on("response", (response) => {
    if (response.status() >= 400 && !response.url().includes("favicon") && !SERVER_PATH.test(response.url()))
      report.failedRequests.push(`${response.request().method()} ${response.url()} ${response.status()}`.slice(0, 300));
  });

  const finding = (text: string) => {
    if (!report.findings.includes(text)) {
      report.findings.push(text);
      log(`  FINDING: ${text}`);
    }
  };
  const shot = async (key: string): Promise<string | null> => {
    if (report.shots.taken >= options.shotCap) return null;
    const name = `${key.replace(/[^\w.-]+/g, "_").slice(0, 80)}.png`;
    if (report.shots.names.includes(name)) return name;
    report.shots.taken++;
    report.shots.names.push(name);
    await page.screenshot({ path: join(options.traceDir, "shots", name) }).catch(() => undefined);
    return name;
  };
  const writeTrace = (file: string, data: unknown, append = false) => {
    const path = join(options.traceDir, file);
    if (append) appendFileSync(path, `${JSON.stringify(data)}\n`);
    else writeFileSync(path, typeof data === "string" ? data : JSON.stringify(data, null, 2));
  };
  const readSave = async (): Promise<any> => page.evaluate(() => (window as any).__sweepSave ?? null).catch(() => null);
  const flush = async (outcome: Outcome, failure?: string) => {
    report.outcome = outcome;
    report.finished = outcome === "finished";
    report.seconds = Math.round((Date.now() - started) / 1000);
    report.decisionsPerMinute = report.seconds > 0 ? Math.round((report.decisions * 600) / report.seconds) / 10 : 0;
    report.heapMb = await page
      .evaluate(() => ((performance as any).memory?.usedJSHeapSize ?? 0) / 1048576)
      .then((v) => Math.round(v))
      .catch(() => null);
    const save = await readSave();
    if (save) {
      writeTrace("save.json", save);
      report.finalDecisionsHash = save.decisions_hash ?? null;
      const stored = currentGameId ? await storedRecord(page, currentGameId).catch(() => null) : null;
      // the full sha-256 of the engine file (what a saved game records), else the 8 digits the page shows
      report.engine = { ...(save.engine ?? {}), wasmSha256: stored?.wasmSha256 ?? (await readWasmHash(page)) };
      if (!report.humanSeat && typeof save.human_seat === "number") report.humanSeat = save.human_seat + 1;
    }
    report.finalStandings = players.map((p) => `${p.faction} ${p.vp ?? "?"} VP`);
    report.factions = players.map((p) => p.faction);
    report.finalStatus = ask ? { kind: (ask as AskSummary).statusKind, round: (ask as AskSummary).round, phase: (ask as AskSummary).phase } : null;
    if (failure) writeTrace("failure.txt", `${failure}\nreport: ${JSON.stringify({ ...report, subtypes: undefined })}\n`);
    writeTrace("browser-errors.json", [
      ...report.pageErrors.map((e) => `pageerror: ${e}`),
      ...report.consoleErrors.map((e) => `console: ${e}`),
      ...report.panics,
      ...report.failedRequests.map((e) => `request: ${e}`),
    ]);
    writeTrace("report.json", report);
    return report;
  };

  // --- setup -----------------------------------------------------------------------------
  try {
    await page.goto(`${options.baseUrl}/offline`, { waitUntil: "domcontentloaded" });
    await page.getByTestId("offline-nickname").fill("Sweeper");
    await page.getByTestId("offline-player-count").selectOption(String(options.players));
    const seat = options.seat === "random" ? String(1 + Math.floor(rng() * options.players)) : options.seat;
    if (options.seat !== "dealt") await page.getByTestId("offline-seat").selectOption(seat === "0" ? "random" : seat);
    if (options.faction !== "dealt") {
      await page.getByTestId("offline-faction-button").click();
      const ids = await page
        .locator('[data-testid^="offline-faction-option-"]')
        .evaluateAll((els) => els.map((el) => el.getAttribute("data-testid")!.replace("offline-faction-option-", "")));
      const roster = ids.filter((id) => id !== "random");
      const wanted = options.faction === "random" ? roster[Math.floor(rng() * roster.length)] : options.faction;
      report.humanFaction = wanted;
      await page.getByTestId(`offline-faction-option-${wanted}`).click();
    }
    await page.getByTestId("offline-start").click();
    await page.waitForSelector('[data-testid="offline-game"]', { timeout: 90_000 });
  } catch (error) {
    await shot("setup-failed");
    const text = await page.locator("body").innerText().catch(() => "");
    return flush("setup-failed", `setup failed: ${String(error).split("\n")[0]}\nPage text: ${text.slice(0, 800)}`);
  }
  const gameId = /\/offline\/games\/([^/?#]+)/.exec(page.url())?.[1] ?? "unknown";
  currentGameId = gameId;
  writeTrace("game.json", { gameId, options: { ...options, log: undefined } });
  log(`game ${gameId} seed=${options.gameSeed} clickSeed=${options.clickSeed} players=${options.players} policy=${options.policy}`);

  // --- the game ----------------------------------------------------------------------------
  let handledSeq = 0;
  let failure: string | undefined;
  let outcome: Outcome = "decision-cap";
  let recoveries = 0;
  let layoutChecks = 0;
  let resumeChecked = false;
  let nextLayoutCheckAt = 1;
  const stallMs = options.stallSeconds * 1000;
  const deadline = started + options.maxSeconds * 1000;
  const seenContainers = new Set<string>();

  const stuck = async (what: string, current: AskSummary) => {
    const name = await shot(`stuck-d${report.decisions}-${current.subtype}`);
    const visible = await page
      .evaluate((ids) => ids.filter((id) => document.querySelector(`[data-testid="${id}"]`) !== null), DECISION_CONTAINERS)
      .catch(() => [] as string[]);
    const text = await page
      .evaluate(() => (document.querySelector('[data-testid="pending-choice-dialog"], [data-testid="turn-action-bar"]') as HTMLElement | null)?.innerText ?? "")
      .catch(() => "");
    const controls = await page
      .evaluate((ids) => {
        const out: string[] = [];
        for (const id of ids) {
          for (const container of Array.from(document.querySelectorAll(`[data-testid="${id}"]`))) {
            for (const el of Array.from(container.querySelectorAll("button, [role='button'], input")).slice(0, 40)) {
              const rect = el.getBoundingClientRect();
              const x = rect.left + rect.width / 2;
              const y = rect.top + rect.height / 2;
              const top = x >= 0 && y >= 0 && x < window.innerWidth && y < window.innerHeight ? document.elementFromPoint(x, y) : null;
              const hidden = top && !el.contains(top) && !top.contains(el) ? `${top.getAttribute("data-testid") ?? top.tagName.toLowerCase()}.${String(top.className).slice(0, 30)}` : "";
              out.push(
                `${id}>${el.getAttribute("data-testid") ?? ""}[${(el.textContent ?? "").trim().replace(/\s+/g, " ").slice(0, 24)}]` +
                  `${(el as HTMLButtonElement).disabled ? " disabled" : ""}${el.getAttribute("aria-disabled") === "true" ? " aria-disabled" : ""}` +
                  `${rect.width === 0 ? " zero-size" : ""}${hidden ? ` covered-by ${hidden}` : ""}`,
              );
            }
          }
        }
        return out;
      }, DECISION_CONTAINERS)
      .catch(() => [] as string[]);
    // Evidence for a stuck question that a covering panel causes: the panel's markup and the chain of the close button.
    if (/system-inspector|close-inspector/.test(await page.content().catch(() => ""))) {
      const dom = await page
        .evaluate(() => {
          const x = document.querySelector('[data-testid="close-inspector-button"]') as HTMLElement | null;
          if (!x) return "no close-inspector-button";
          const r = x.getBoundingClientRect();
          const top = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2) as HTMLElement | null;
          const chain: string[] = [];
          for (let e: HTMLElement | null = x; e && chain.length < 12; e = e.parentElement)
            chain.push(`${e.tagName.toLowerCase()}${e.id ? `#${e.id}` : ""}.${String(e.className).slice(0, 40)}[${e.getAttribute("data-testid") ?? ""}] pointer-events=${getComputedStyle(e).pointerEvents} z=${getComputedStyle(e).zIndex} position=${getComputedStyle(e).position}`);
          return `x rect ${Math.round(r.left)},${Math.round(r.top)} ${Math.round(r.width)}x${Math.round(r.height)}; element at its centre: ${top ? `${top.tagName.toLowerCase()}.${String(top.className).slice(0, 40)}[${top.getAttribute("data-testid") ?? ""}]` : "none"}\n${chain.join("\n")}`;
        })
        .catch(() => "");
      writeTrace("inspector-dom.txt", dom);
    }
    const closers = await page
      .evaluate(() =>
        Array.from(document.querySelectorAll('[data-testid*="close"], [aria-label*="lose"]'))
          .slice(0, 8)
          .map((el) => {
            const r = el.getBoundingClientRect();
            const top = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
            return `${el.getAttribute("data-testid") ?? el.getAttribute("aria-label")} ${Math.round(r.left)},${Math.round(r.top)} ${Math.round(r.width)}x${Math.round(r.height)}${top && !el.contains(top) ? ` covered-by ${top.getAttribute("data-testid") ?? top.tagName}` : ""}`;
          }),
      )
      .catch(() => [] as string[]);
    finding(
      `stuck: ${what} at decision #${report.decisions}, round ${current.round}, ${current.phase}, subtype ${current.subtype}, prompt "${current.prompt}"; ` +
        `options ${JSON.stringify(current.options.map((o) => o.id)).slice(0, 400)}; containers on screen ${JSON.stringify(visible)}; ` +
        `controls ${JSON.stringify(controls).slice(0, 1500)}; close controls ${JSON.stringify(closers)}; visible text "${text.replace(/\s+/g, " ").slice(0, 300)}"${name ? `; screenshot shots/${name}` : ""}`,
    );
  };

  const layoutCheck = async (current: AskSummary) => {
    layoutChecks++;
    const problems = await page
      .evaluate((ids) => {
        const out: string[] = [];
        const root = document.documentElement;
        if (root.scrollWidth > window.innerWidth + 6) out.push(`page scrolls sideways (${root.scrollWidth}px wide in a ${window.innerWidth}px window)`);
        for (const id of ids) {
          for (const el of Array.from(document.querySelectorAll(`[data-testid="${id}"]`))) {
            const rect = el.getBoundingClientRect();
            if (rect.width === 0 || rect.height === 0) continue;
            if (rect.right > window.innerWidth + 8 || rect.left < -8)
              out.push(`${id} sticks out horizontally (${Math.round(rect.left)}..${Math.round(rect.right)} in a ${window.innerWidth}px window)`);
          }
        }
        return out;
      }, DECISION_CONTAINERS)
      .catch(() => [] as string[]);
    for (const problem of problems) {
      const name = await shot(`layout-d${report.decisions}-${current.subtype}`);
      finding(`layout: ${problem} at decision #${report.decisions}, round ${current.round}, subtype ${current.subtype}${name ? `; screenshot shots/${name}` : ""}`);
    }
  };

  try {
    while (true) {
      if (crashed) {
        outcome = "crash";
        failure = `engine crash: ${crashed}`;
        break;
      }
      if (ended) {
        outcome = "finished";
        break;
      }
      if (report.decisions >= options.maxDecisions) {
        outcome = "decision-cap";
        break;
      }
      if (Date.now() > deadline) {
        outcome = "time-cap";
        break;
      }
      const current = ask as AskSummary | null;
      if (!current || current.seq <= handledSeq) {
        // the engine/bots are thinking, or the game is over
        if (Date.now() - lastMessageAt > stallMs) {
          const last = current ?? ({ subtype: "none", prompt: "", round: null, phase: null, options: [] } as unknown as AskSummary);
          await stuck(`the engine sent nothing for ${options.stallSeconds} s after decision #${report.decisions}`, last);
          if (recoveries < 1) {
            recoveries++;
            report.recoveries = recoveries;
            lastMessageAt = Date.now();
            log("  recovery: reload the page (resume)");
            // Forget the old question BEFORE the reload: the resumed engine's question can arrive before the game
            // screen is up, and clearing afterwards threw it away (a false "engine sent nothing for 90 s", F3).
            handledSeq = 0; // the resumed game asks again
            ask = null;
            await page.reload({ waitUntil: "domcontentloaded" }).catch(() => undefined);
            await page.waitForSelector('[data-testid="offline-game"]', { timeout: 120_000 }).catch(() => undefined);
            lastMessageAt = Date.now();
            continue;
          }
          outcome = "stuck";
          failure = `the engine sent nothing for ${options.stallSeconds} s (decision #${report.decisions})`;
          break;
        }
        report.timing.idle += 40;
        await sleep(40);
        continue;
      }

      // ---- one decision -----------------------------------------------------------------
      // Resume check (TI4_OFFLINE_RESUME_AT=<decision>): the player closes the tab and opens the game again at
      // this question. The page must show the same question again (same game version) within a minute.
      if (options.resumeAt !== null && !resumeChecked && report.decisions >= options.resumeAt) {
        resumeChecked = true;
        // the store writes the position of each question as it is asked; let the last write land first (the UI can
        // answer a lone option by itself meanwhile, so the position to compare with is the one after the wait)
        await sleep(1500);
        const settled = (ask as AskSummary | null) ?? current;
        const before = { version: settled.version, answers: settled.answers, subtype: settled.subtype };
        const t0 = Date.now();
        handledSeq = 0;
        ask = null;
        await page.reload({ waitUntil: "domcontentloaded" }).catch(() => undefined);
        while (!ask && Date.now() - t0 < 60_000 && !ended && !crashed) await sleep(100);
        const resumed = ask as AskSummary | null;
        report.resume = {
          at: report.decisions,
          answers: before.answers,
          ms: Date.now() - t0,
          ok: !!resumed && resumed.answers === before.answers && resumed.subtype === before.subtype,
        };
        if (!resumed) {
          const name = await shot(`resume-hang-d${report.decisions}`);
          const text = await page.locator("body").innerText().catch(() => "");
          finding(
            `resume: reloading the page at decision #${report.decisions} (${before.answers} answers, round ${current.round}, subtype ${before.subtype}) never showed the game again within 60 s; ` +
              `the page says "${text.replace(/\s+/g, " ").slice(90, 250)}"${name ? `; screenshot shots/${name}` : ""}`,
          );
          outcome = "stuck";
          failure = `resume hang at decision #${report.decisions}`;
          break;
        }
        if (!report.resume.ok)
          finding(`resume: after a reload at decision #${report.decisions} the page asked ${resumed.subtype} at ${resumed.answers} answers, not ${before.subtype} at ${before.answers}`);
        log(`  resume check at decision #${report.decisions}: ${report.resume.ok ? "ok" : "DIFFERENT"} in ${report.resume.ms} ms`);
        continue;
      }
      const waited = Date.now() - lastMessageAt;
      const subtype = current.subtype;
      const hexWeights =
        options.policy === "steer" && subtype === "activate_system" && current.board && current.player
          ? activationWeights(current.board, current.player, false)
          : new Map<string, number>();
      report.subtypes[subtype] = (report.subtypes[subtype] ?? 0) + 1;
      if (current.round !== null && !(current.round in report.roundStarts)) {
        report.roundStarts[current.round] = report.decisions;
        log(`round ${current.round} reached after ${report.decisions} decisions (${Math.round((Date.now() - started) / 1000)} s)`);
        await shot(`round-${current.round}-d${report.decisions}-${subtype}`);
      }
      writeTrace(
        "trace.jsonl",
        {
          decision: report.decisions,
          version: current.version,
          round: current.round,
          phase: current.phase,
          seat: report.humanSeat ?? 1,
          player: current.player,
          faction: players.find((p) => p.id === current.player)?.faction,
          subtype,
          source: current.source,
          prompt: current.prompt,
          options: current.options,
        },
        true,
      );
      const isReaction = isReactionSubtype(subtype);
      let reactionChecked = !isReaction;
      let clicks = 0;
      let emptyPolls = 0;
      let answered = false;
      let reloaded = false;
      const maxClicks = Number(process.env.TI4_OFFLINE_MAX_CLICKS ?? 80);
      const answeredNow = () => ((ask as AskSummary | null)?.seq ?? 0) > current.seq || !!ended || !!crashed;

      while (!answered && !answeredNow()) {
        if (Date.now() > deadline) break;
        if (clicks >= maxClicks || emptyPolls > Math.max(100, (stallMs / 1000) * 5)) {
          await stuck(clicks >= maxClicks ? `${clicks} clicks did not answer the question` : `no actionable control`, current);
          // Recovery 1: Escape and the unstage controls; recovery 2: reload (resume replays the answers).
          if (recoveries < 3) {
            recoveries++;
            report.recoveries = recoveries;
            log(`  recovery ${recoveries}: ${recoveries === 1 ? "escape" : "reload the page (resume)"}`);
            if (recoveries === 1) {
              await page.keyboard.press("Escape").catch(() => undefined);
            } else {
              // Clear before the reload (see the stall recovery above): the resumed question may arrive early.
              handledSeq = 0;
              ask = null;
              await page.reload({ waitUntil: "domcontentloaded" }).catch(() => undefined);
              await page.waitForSelector('[data-testid="offline-game"]', { timeout: 120_000 }).catch(() => undefined);
              lastMessageAt = Date.now();
              reloaded = true;
              break;
            }
            clicks = 0;
            emptyPolls = 0;
            continue;
          }
          outcome = "stuck";
          failure = `stuck on ${subtype} at decision #${report.decisions} after ${recoveries} recoveries`;
          break;
        }
        if (!reactionChecked) {
          const shown = await page.getByTestId("reaction-status-bar").first().textContent({ timeout: 300 }).catch(() => null);
          if (shown !== null) {
            reactionChecked = true;
            const problem = reactionTextProblem(shown);
            if (problem) {
              const name = await shot(`reaction-text-d${report.decisions}-${subtype}`);
              finding(`reaction text: ${subtype}: ${problem}${name ? `; screenshot shots/${name}` : ""}`);
            }
          }
        }
        if (report.decisions >= nextLayoutCheckAt && !answeredNow()) {
          nextLayoutCheckAt = report.decisions + 20;
          await layoutCheck(current);
        }
        const scanStart = Date.now();
        const candidates = await collectCandidates(page).catch((err: Error) => {
          if (emptyPolls % 20 === 0) log(`  candidate scan failed: ${err.message.split("\n")[0]}`);
          return [] as Awaited<ReturnType<typeof collectCandidates>>;
        });
        report.timing.scan += Date.now() - scanStart;
        if (!candidates.length) {
          emptyPolls++;
          // A system inspector (opened by tapping a hex) or a turn bar menu can cover the question; closing it is a
          // normal player move (counted). A notification covering a control is a finding, and click-through.
          if (emptyPolls === 8 || emptyPolls === 22) {
            const closed = await dismissOverlays(page);
            if (closed.length) {
              report.overlayDismissals += closed.length;
              emptyPolls = 0;
              continue;
            }
          }
          if (emptyPolls === 6 || emptyPolls === 20) {
            // The mouse rests where the last click was; a hover card of a unit chip that came to lie under it can sit
            // over the only control. A player moves the pointer away.
            await page.mouse.move(1, 1).catch(() => undefined);
          }
          if (emptyPolls === 14 || emptyPolls === 30) {
            // Controls of the question that are covered by something else (a toast history, a banner): a real
            // player would close or move the cover; the harness makes the cover click-through, and says so.
            const covers = await unblockCovered(page);
            if (covers.length) {
              const name = await shot(`overlap-d${report.decisions}-${subtype}`);
              for (const cover of covers)
                finding(`overlap: ${cover} at decision #${report.decisions}, round ${current.round}, subtype ${subtype}${name ? `; screenshot shots/${name}` : ""}`);
              emptyPolls = 0;
              continue;
            }
          }
          if (emptyPolls === 8) {
            const close = page.locator('[data-testid="close-inspector-button"]').first();
            if ((await close.count().catch(() => 0)) > 0 && (await close.click({ timeout: 1_000 }).then(() => true).catch(() => false))) {
              report.overlayDismissals++;
              emptyPolls = 0;
              continue;
            }
          }
          await sleep(100);
          continue;
        }
        emptyPolls = 0;
        if (clicks > 0 && clicks % 10 === 0 && (await escapePaymentDeadEnd(page))) {
          log(`#${report.decisions} ${subtype} escape: the payment plan cannot be paid; asked for each build`);
          clicks++;
          report.clicks++;
          continue;
        }
        const chosen = pick(
          preferTechWindowUse(subtype, current.prompt, candidates, 0, true),
          clicks,
          rng,
          options.policy,
          hexWeights,
        );
        log(`#${report.decisions} ${subtype} click ${chosen.desc}`);
        const clickStart = Date.now();
        await page
          .locator(`[data-smoke-idx="${chosen.idx}"]`)
          .click({ timeout: 2_000 })
          .catch((err: Error) => log(`  click failed: ${err.message.split("\n")[0]}`));
        report.timing.click += Date.now() - clickStart;
        clicks++;
        report.clicks++;
        if (chosen.commit) {
          // A committing click normally gets its answer to the engine at once; the bots then play on and the
          // next question arrives within a second or two. A click that does not commit (a staging step that
          // looks like a confirm) must not hold the loop for long: scan again after COMMIT_WAIT_MS.
          const commitStart = Date.now();
          // Controls that often did not commit (an empty "Propose Deal") only get a short wait.
          const key = chosen.desc.split(" | ")[0].replace(/[0-9]+/g, "N").slice(0, 40);
          const until = commitStart + ((report.commitTimeoutControls[key] ?? 0) >= 2 ? Math.min(250, COMMIT_WAIT_MS) : COMMIT_WAIT_MS);
          while (!answeredNow() && Date.now() < until) await sleep(15);
          const waitedMs = Date.now() - commitStart;
          report.timing.commit += waitedMs;
          if (!answeredNow()) {
            report.commitTimeouts++;
            report.commitTimeoutControls[key] = (report.commitTimeoutControls[key] ?? 0) + 1;
          }
        } else {
          await sleep(40);
        }
        if (answeredNow()) break;
        for (const error of await visibleErrors(page)) {
          const entry = `${subtype}: ${error}`;
          if (!report.rejections.includes(entry)) {
            report.rejections.push(entry);
            const name = await shot(`error-banner-d${report.decisions}-${subtype}`);
            finding(`error banner: ${entry.slice(0, 300)}${name ? `; screenshot shots/${name}` : ""}`);
          }
        }
      }
      if (outcome === "stuck") break;
      if (reloaded) continue;
      handledSeq = current.seq;
      report.decisions++;
      report.engineWaitMs += waited > 0 && waited < 600_000 ? waited : 0;
      report.worstWaitMs = Math.max(report.worstWaitMs, waited);
      if (report.decisions % 100 === 0)
        log(`  ${report.decisions} decisions, round ${current.round}, ${Math.round((Date.now() - started) / 1000)} s, console errors ${report.consoleErrors.length}`);
    }
  } catch (error) {
    failure = `harness error: ${String(error instanceof Error ? error.stack ?? error.message : error).slice(0, 1500)}`;
    outcome = "stuck";
    await shot("harness-error");
  }

  // --- the end -----------------------------------------------------------------------------
  if (outcome === "finished") {
    const banner = await settledGameOverBanner(() =>
      page.getByTestId("turn-status-banner").textContent({ timeout: 2_000 }),
    );
    if (!/Game Over/i.test(banner ?? "")) {
      const name = await shot("no-game-over-banner");
      finding(`the engine ended the game but the status banner says "${(banner ?? "").slice(0, 80)}"${name ? `; screenshot shots/${name}` : ""}`);
    } else {
      await shot("game-over");
    }
  }
  // Leftover visible error banners at the end.
  void ERROR_BANNERS;
  if (crashed && outcome !== "crash") {
    outcome = "crash";
    failure ??= `engine crash: ${crashed}`;
  }
  if (outcome === "crash" || outcome === "stuck") await shot(`final-${outcome}`);
  return flush(outcome, failure);
}

/**
 * Makes a notification that covers an enabled control of the pending question click-through (the cover
 * stays a finding: a player would have to close it). Intended overlays are left alone: a menu or panel of
 * the turn bar, a dialog or modal, the system inspector (those are closed, see `dismissOverlays`).
 * Only the covering element and the small containers around it are touched, never a structural container.
 * Returns one line per cover ("<control> is covered by <element>").
 */
async function unblockCovered(page: Page): Promise<string[]> {
  return page
    .evaluate((ids) => {
      const out: string[] = [];
      const intended = '[data-testid="turn-action-bar"], [role="dialog"], [data-testid$="-modal"], [data-testid="system-inspector"], [data-testid="close-inspector-button"], [data-testid="player-sheet"]';
      const area = window.innerWidth * window.innerHeight;
      for (const id of ids) {
        for (const container of Array.from(document.querySelectorAll(`[data-testid="${id}"]`))) {
          for (const el of Array.from(container.querySelectorAll("button, [role='button']")).slice(0, 60)) {
            const button = el as HTMLButtonElement;
            if (button.disabled || el.getAttribute("aria-disabled") === "true") continue;
            const rect = el.getBoundingClientRect();
            if (rect.width === 0 || rect.height === 0) continue;
            const x = rect.left + rect.width / 2;
            const y = rect.top + rect.height / 2;
            if (x < 0 || y < 0 || x >= window.innerWidth || y >= window.innerHeight) continue;
            const top = document.elementFromPoint(x, y) as HTMLElement | null;
            if (!top || el.contains(top) || top.contains(el)) continue;
            if (top.closest(intended)) continue;
            // the cover and the small containers around it (a toast column), never a big structural parent
            let cover: HTMLElement = top;
            while (cover.parentElement && cover.parentElement !== document.body && !cover.parentElement.contains(container)) {
              const pr = cover.parentElement.getBoundingClientRect();
              if (pr.width * pr.height > area * 0.2) break;
              cover = cover.parentElement;
            }
            const label = (e: Element) => `${e.getAttribute("data-testid") ?? e.tagName.toLowerCase()}${e.className ? `.${String(e.className).split(" ")[0]}` : ""}`;
            out.push(`${el.getAttribute("data-testid") ?? (el.textContent ?? "").trim().slice(0, 20)} (${id}) is covered by ${label(top)}${cover !== top ? ` inside ${label(cover)}` : ""}`);
            cover.style.pointerEvents = "none";
            top.style.pointerEvents = "none";
          }
        }
      }
      return Array.from(new Set(out));
    }, DECISION_CONTAINERS)
    .catch(() => [] as string[]);
}

/** Closes what a player would close to get back to the question: the system inspector, a turn bar menu. Returns what it closed. */
async function dismissOverlays(page: Page): Promise<string[]> {
  const closed: string[] = [];
  for (const testId of OVERLAY_CLOSE_IDS) {
    const button = page.locator(`[data-testid="${testId}"]`).first();
    if ((await button.count().catch(() => 0)) > 0 && (await button.click({ timeout: 1_000 }).then(() => true).catch(() => false))) closed.push(testId);
  }
  return closed;
}

async function readWasmHash(page: Page): Promise<string | undefined> {
  const text = await page.getByTestId("offline-engine").textContent({ timeout: 500 }).catch(() => null);
  return text?.replace(/^engine\s+/, "") ?? undefined;
}
