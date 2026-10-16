import { appendFileSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  expect,
  type APIRequestContext,
  type Browser,
  type Page,
} from "@playwright/test";
import {
  createStartedGame,
  gameSnapshot,
  openPlayerGame,
} from "./lobbyHelpers";
import type { BoardView } from "../src/protocol/types";
import { startupRetryable, tolerableConsoleError } from "./smokeBrowserErrors";
import { basePreset, missingExpected, type Expectation } from "./smokePreset";
import { SecondaryPrepExercise, emptyPrepReport, prepConfigFromEnv, type PrepReport } from "./smokePrep";
import {
  RedoExercise,
  UiTour,
  exerciseFlagsFromEnv,
  readRecapToasts,
  readToastKinds,
  recapInitScript,
  shotCapFromEnv,
  type ExerciseReport,
} from "./smokeExercises";
import {
  activationWeight,
  coveredBy,
  preferHitConfirm,
  preferPayment,
  preferReadyConfirm,
  preferTechWindowUse,
  preferTokenConfirm,
  preferTradeDesk,
  steerWeight as policySteerWeight,
  strongUnselected,
  classifyControl,
  serverProgressed,
  classifyCandidate,
  isBarControl,
  turnBarPool,
  pausedBatch,
  EXCLUDED_CONTROLS,
  isReactionSubtype,
  reactionTextProblem,
} from "./smokePolicy";

/**
 * Random UI playthrough: every pending decision is resolved by clicking randomly among the
 * controls the web UI offers to the acting seat. The server is only read to find the acting
 * seat and to detect progress; every action goes through the browser.
 */
export interface PlaythroughOptions {
  playerCount: number;
  gameSeed: number;
  clickSeed: number;
  /** Start preset for the game (e.g. "combat": fleets beside homes and Mecatol). */
  startPreset?: string;
  /** Strategy card set for the game ("te" server default, "pok", "base_game_codex1"). */
  cardSet?: string;
  /** Named map template for the game (default: the server's choice for the table size). */
  mapTemplate?: string;
  /** Decision subtypes the run must have offered (see `parseExpect`); a miss fails the run. */
  expect?: Expectation[];
  /** Stop successfully after this many resolved decisions. */
  maxDecisions: number;
  /** Fail when a single decision does not advance after this many clicks. */
  maxClicksPerDecision: number;
  /** Stop successfully once the game reaches this round. */
  stopAtRound?: number;
  /** `random` clicks uniformly; `steer` favours tactical play so combat, invasion and agendas occur. */
  policy?: "random" | "steer";
  log?: (line: string) => void;
  /**
   * When set, write a machine-readable trace here: `game.json` (id and seats) at the start,
   * `trace.jsonl` (one line per offered decision), and `report.json` + `final-snapshot.json`
   * at the end. Used by the nightly proctors to compile what happened in a game.
   */
  traceDir?: string;
}

export interface PlaythroughReport {
  gameId: string;
  startPreset: string | null;
  cardSet: string | null;
  decisions: number;
  clicks: number;
  finished: boolean;
  /** Decisions resolved before each round was first seen, keyed by round. */
  roundStarts: Record<number, number>;
  finalStatus: unknown;
  subtypes: Record<string, number>;
  /** Error banners the UI showed after a click, usually server rejections of offered controls. */
  rejections: string[];
  /** Batches the server stopped at a reaction window (not failures). */
  pausedBatches: number;
  /** Action-phase decisions (the turn menu and the end-turn question) seen in the run. */
  turnMenuDecisions: number;
  /** Of those, resolved with a click on the persistent turn bar. */
  barDecisions: number;
  /** Bar clicks that submitted, by control (`turn-bar-strategic-<card>`, `turn-bar-open`, ...). */
  barControls: Record<string, number>;
  /** Turn-menu decisions that fell back to the old list because the bar could not map them. */
  turnMenuFallbacks: string[];
  /** Reaction decisions (windows, the inner card pick, Instinct Training, the L1Z1X agent) seen. */
  reactionDecisions: number;
  /** Reaction dialogs whose text showed a raw engine id or a doubled verb ("Play play"). */
  reactionTextProblems: string[];
  /** Expectations (`TI4_SMOKE_EXPECT`) the run did not meet; empty when all were met. */
  expectMissing: string[];
  /** Optional exercises (UI tour, recap toasts, turn redo) that ran in this run, and what they saw. */
  exercises: ExerciseReport;
  /** Secondary pre-planning exercise counters (opportunities, planned, auto-played, ...). */
  prep: PrepReport;
  /** Problems the exercises found (empty/clipped info cards, redo aborts, ...). Not run failures. */
  findings: string[];
  /** Screenshots saved to `trace/shots`, and the cap that applied. */
  shots: { taken: number; cap: number; names: string[] };
  /**
   * Where the harness spent its wall time (TI4_SMOKE_TIMING=1): per phase the summed milliseconds
   * and the number of calls. `status` and `actor` are the two snapshot GETs of every loop, `ui`
   * the wait for the tab to catch up, `scan` the control scan, `click` the Playwright click,
   * `commit` the wait for the new version after a committing click, `settle` the pause after a
   * staging click, `prep` and `tour` the optional exercises.
   */
  timing?: Record<string, { ms: number; calls: number }>;
  /** Per seat, with TI4_SMOKE_TIMING=1: requests the page made until the status bar showed, and how long that took. */
  pageLoads?: { seat: number; ms: number; requests: number; scripts: number }[];
}

// Containers that render a decision for the acting seat.
const DECISION_CONTAINERS = [
  "turn-action-bar",
  "pending-choice-dialog",
  "system-activation-bar",
  "planet-selection-bar",
  "ready-planets-bar",
  "tactical-movement-tray",
  "cargo-loading-tray",
  "invasion-landing-tray",
  "invasion-overlay",
  "payment-drawer",
  "payment-bar",
  "token-payment-bar",
  "production-builder-drawer",
  "objectives-modal",
  "technology-modal",
  "combat-resolution-modal",
  "trade-desk-modal",
  "agenda-ballot-modal",
  "reaction-status-bar",
  "paused-plan",
];

// New UI surfaces worth a look by the proctor: the first time one is on screen during a run (and
// the first decision of each round), a screenshot of the acting seat's tab lands in
// `<traceDir>/shots/`. A handful per run, so the disk and the proctor's reading stay small.
const SHOT_TARGETS = [
  "trade-staging-desk",
  "trade-offer-columns",
  "production-builder-drawer",
  "token-payment-bar",
  "command-token-panel",
  "secondary-prep",
  "combat-result-summary",
  "ground-combat-result-summary",
  "corner-toast",
  "objectives-modal",
  "agenda-ballot-modal",
];
// Cap on saved screenshots per run (TI4_SMOKE_SHOT_CAP, default 20, "all" = 20). The exercise shots
// (tour, redo) count inside the cap and only fit while there is room left.


const ERROR_BANNERS = [
  "choice-error-banner",
  "movement-error-banner",
  "activation-error",
  "planet-selection-error",
  "ready-bar-error",
  "reaction-error-badge",
  "combat-error-banner",
  "hit-assignment-error",
  "token-error",
];

// Controls that hide the decision or rewrite history; clicking them never advances the game.
const EXCLUDED = EXCLUDED_CONTROLS;
// Controls that take back staged selections. Only used to escape a staging dead end, such as
// cargo over transport capacity, where every submit button is disabled.
interface Candidate {
  idx: number;
  desc: string;
  /** Untruncated description used for steering. */
  full: string;
  resume: boolean;
  commit: boolean;
  unstage: boolean;
  checked: boolean;
}

function mulberry32(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export async function collectCandidates(page: Page): Promise<Candidate[]> {
  const raw = await page.evaluate(
    ({ containers, excluded, coveredSrc }) => {
      const excludedRe = new RegExp(excluded, "i");
      const hiddenBehind = new Function(`return (${coveredSrc})`)() as (
        el: Element,
        hit: Element | null,
      ) => boolean;
      document
        .querySelectorAll("[data-smoke-idx]")
        .forEach((el) => el.removeAttribute("data-smoke-idx"));
      // A board hex behind a modal is rendered but cannot receive the click. Only on-screen
      // centers are tested, because Playwright scrolls off-screen controls into view itself (also
      // a control clipped by its own scrolling dialog, see coveredBy).
      const covered = (rect: DOMRect, el: Element) => {
        const x = rect.left + rect.width / 2;
        const y = rect.top + rect.height / 2;
        if (x < 0 || y < 0 || x >= window.innerWidth || y >= window.innerHeight)
          return false;
        return hiddenBehind(el, document.elementFromPoint(x, y));
      };
      const visible = (el: Element) => {
        const rect = el.getBoundingClientRect();
        const style = getComputedStyle(el);
        return (
          rect.width > 0 &&
          rect.height > 0 &&
          style.visibility !== "hidden" &&
          style.pointerEvents !== "none" &&
          !covered(rect, el)
        );
      };
      // The trade desk's "Quick deals" list sits in a closed <details>: its buttons have a box but
      // cannot be clicked until the summary is opened, so the click only timed out.
      const folded = (el: Element) => {
        const closed = el.closest("details:not([open])");
        return closed !== null && !el.closest("summary");
      };
      const enabled = (el: Element) =>
        !(el as HTMLButtonElement).disabled &&
        el.getAttribute("aria-disabled") !== "true" &&
        el.getAttribute("data-actionable") !== "false";
      // Secondary preparation (chip, banner, prepared-answer bar) answers into a local plan, never
      // the engine's question; it must not be clicked as a decision control.
      const inPrep = (el: Element) => el.closest('[data-testid="secondary-prep"]') !== null;
      // Corner toasts (recap, action, auto-resolve) are buttons that dismiss on click; they are
      // notifications, never decision controls.
      const inToast = (el: Element) =>
        el.closest('[data-testid="corner-toasts"], [data-testid="corner-toast"]') !== null;
      const found = new Set<Element>();
      document
        .querySelectorAll(
          '[data-testid^="resume-"], [data-testid="choice-minimized-pill"] button, [data-target-candidate="true"]',
        )
        .forEach((el) => found.add(el));
      for (const id of containers) {
        document
          .querySelectorAll(`[data-testid="${id}"]`)
          .forEach((container) => {
            container
              .querySelectorAll(
                'button, [role="button"], input[type="checkbox"], input[type="radio"], [data-testid="choice-option"], [data-selectable="true"]',
              )
              .forEach((el) => {
                // A choice option label already covers its inner radio or checkbox.
                if (
                  el.closest('[data-testid="choice-option"]') !== el &&
                  el.closest('[data-testid="choice-option"]')
                )
                  return;
                found.add(el);
              });
          });
      }
      const out: {
        idx: number;
        desc: string;
        full: string;
        resume: boolean;
        checked: boolean;
        staged: boolean;
      }[] = [];
      let idx = 0;
      for (const el of found) {
        if (!visible(el) || !enabled(el) || inPrep(el) || inToast(el) || folded(el)) continue;
        // An unlabeled checkbox or radio is described by the label that wraps it.
        const named =
          el instanceof HTMLInputElement ? (el.closest("label") ?? el) : el;
        const testId = named.getAttribute("data-testid") ?? "";
        const label = named.getAttribute("aria-label") ?? "";
        const fullText = (named.textContent ?? "").trim().replace(/\s+/g, " ");
        const text = fullText.slice(0, 60);
        const desc = [testId, label, text].filter(Boolean).join(" | ");
        const full = [testId, label, fullText].filter(Boolean).join(" | ");
        if (excludedRe.test(testId) || excludedRe.test(label)) continue;
        if (!testId && excludedRe.test(text)) continue;
        el.setAttribute("data-smoke-idx", String(idx));
        out.push({
          idx,
          desc,
          full,
          resume:
            testId.startsWith("resume-") ||
            el.closest('[data-testid="choice-minimized-pill"]') !== null,
          checked: el instanceof HTMLInputElement && el.checked,
          // A selected technology card: clicking it again takes the pick back.
          staged: el.matches('[data-testid^="tech-card-"][data-selected="true"]'),
        });
        idx++;
      }
      return out;
    },
    { containers: DECISION_CONTAINERS, excluded: EXCLUDED.source, coveredSrc: coveredBy.toString() },
  );
  return raw.map(({ staged, ...c }) => {
    return { ...c, ...classifyCandidate(c.desc, staged) };
  });
}

// Base move values; faction variants such as `sol_carrier` share the suffix. Ground forces,
// structures and fighters cannot move on their own.
function shipMove(unitType: string): number {
  if (/(carrier|dreadnought|flagship)$/.test(unitType)) return 1;
  if (/(cruiser|destroyer|warsun|war_sun)$/.test(unitType)) return 2;
  return 0;
}

/**
 * Activation targets worth steering toward: systems one of the actor's fleets can reach by base
 * move value (ignoring anomalies and wormholes), weighted up for Mecatol Rex and for systems
 * holding another player's units. The UI does not show reachability, so this reads the actor's
 * board view.
 */
function activationWeights(
  board: BoardView,
  actor: string,
  exploreMode = false,
): Map<string, number> {
  const tiles = new Map((board.map_tiles ?? []).map((t) => [t.system_id, t]));
  const fleets = Object.values(board.systems).flatMap((sys) => {
    const tile = tiles.get(sys.system_id);
    const move = Math.max(
      0,
      ...sys.units
        .filter((u) => u.owner === actor && !u.planet)
        .map((u) => shipMove(u.unit_type)),
    );
    return tile && move > 0 ? [{ tile, move }] : [];
  });
  const weights = new Map<string, number>();
  for (const [id, tile] of tiles) {
    const reachable = fleets.some(({ tile: f, move }) => {
      const distance =
        (Math.abs(f.q - tile.q) +
          Math.abs(f.r - tile.r) +
          Math.abs(f.q + f.r - tile.q - tile.r)) /
        2;
      // Ships already in the active system cannot move, so distance 0 does not count.
      return distance > 0 && distance <= move;
    });
    const enemies =
      board.systems[id]?.units.some((u) => u.owner !== actor) ?? false;
    const defended =
      board.systems[id]?.units.some((u) => u.owner !== actor && !!u.planet) ??
      false;
    const ownGroundInSpace =
      board.systems[id]?.units.some(
        // Still in space: once landed on the planet there is nothing left to do in place.
        (u) =>
          u.owner === actor &&
          !u.planet &&
          /infantry|mech|spec_ops/i.test(u.unit_type),
      ) ?? false;
    // An unclaimed planet with an exploration trait (nobody has units on it): landing explores it.
    const explorable =
      !!tile?.planets?.some((p) => (p.traits?.length ?? 0) > 0 && !p.legendary && !p.space_station) &&
      !(board.systems[id]?.units.some((u) => !!u.planet) ?? false);
    // A planetless system holding the actor's ships (a frontier token with Dark Energy Tap): the
    // explore preset parks a ship pair there, and activating the system explores the token.
    const frontierShips =
      exploreMode &&
      !!tile &&
      !tile.hyperlane &&
      (tile.planets?.length ?? 0) === 0 &&
      id !== "18" &&
      (board.systems[id]?.units.some((u) => u.owner === actor && !u.planet && shipMove(u.unit_type) > 0) ??
        false);
    const inPlace = (id === "18" || defended) && ownGroundInSpace;
    const exploreInPlace = (explorable && ownGroundInSpace) || frontierShips;
    weights.set(
      id,
      activationWeight(id, reachable, enemies, inPlace, defended, exploreInPlace),
    );
  }
  return weights;
}

// The notes preset wants promissory notes sold, swapped and given: trading is steered up there.
let tradeBoost = false;

function steerWeight(desc: string, hexWeights: Map<string, number>): number {
  const hex = /^system-hex-(\S+) /.exec(desc);
  if (hex) return hexWeights.get(hex[1]) ?? 1;
  if (tradeBoost && /^turn-bar-trade\b/.test(desc)) return 6;
  return policySteerWeight(desc);
}

function weightedPick(
  pool: Candidate[],
  weight: (c: Candidate) => number,
  rng: () => number,
) {
  const weights = pool.map(weight);
  let roll = rng() * weights.reduce((a, b) => a + b, 0);
  for (const [i, w] of weights.entries()) {
    roll -= w;
    if (roll < 0) return pool[i];
  }
  return pool[pool.length - 1];
}

function pick(
  candidates: Candidate[],
  clicks: number,
  rng: () => number,
  policy: "random" | "steer",
  hexWeights: Map<string, number>,
): Candidate {
  // The turn bar has no staging step: choose one of its controls by weight (or uniformly).
  const bar = turnBarPool(candidates);
  if (bar) {
    if (policy === "random") return bar[Math.floor(rng() * bar.length)];
    return weightedPick(bar, (c) => steerWeight(c.full, hexWeights), rng);
  }
  candidates = preferTokenConfirm(
    preferTradeDesk(preferHitConfirm(preferReadyConfirm(preferPayment(candidates))), rng),
    rng,
  );
  const resume = candidates.filter((c) => c.resume);
  if (resume.length) return resume[Math.floor(rng() * resume.length)];
  const unstage = candidates.filter((c) => c.unstage);
  const forward = candidates.filter((c) => !c.unstage);
  // Take staging back when nothing else is clickable, now and then when no submit control is
  // enabled (such as cargo over transport capacity), or once a decision has stalled. Dropping
  // cargo one unit at a time keeps the ships staged, so the plan can become committable. The
  // blocked chance stays low: a payment that needs every source (3 owed, exactly 3 available)
  // only unlocks its confirm button after several staging clicks in a row.
  const blocked = !forward.some((c) => c.commit);
  if (
    unstage.length &&
    (!forward.length ||
      (blocked && rng() < 0.1) ||
      (clicks >= 10 && rng() < 0.2))
  )
    return weightedPick(
      unstage,
      (c) => (/cargo|decrement|remove/i.test(c.desc) ? 5 : 0.5),
      rng,
    );
  const commits = forward.filter((c) => c.commit);
  const stages = forward.filter((c) => !c.commit);
  const strong =
    policy === "steer"
      ? strongUnselected(stages.map((c) => ({ ...c, desc: c.full })))
      : undefined;
  if (strong) return stages.find((c) => c.idx === strong.idx) ?? strong;
  // Stage a few selections first, then lean toward submitting as the decision drags on. Steered
  // fleet and landing trays stage longer so ships and ground forces actually move.
  const staging =
    policy === "steer" &&
    stages.some((c) => /^rally-inc-| in space/i.test(c.full));
  const commitChance = staging
    ? Math.min(0.05 + 0.08 * clicks, 0.9)
    : Math.min(0.35 + 0.15 * clicks, 0.9);
  const pool =
    commits.length && (!stages.length || rng() < commitChance)
      ? commits
      : stages;
  if (policy === "random") return pool[Math.floor(rng() * pool.length)];
  return weightedPick(pool, (c) => steerWeight(c.full, hexWeights), rng);
}

/**
 * When a rejected batch was a plan the UI staged (the engine says a planned step is not among the
 * offered options), name the step and the offered ids: the control let the player stage a move
 * the engine never offered, so the bug is in the staging UI rather than the engine.
 */
function stagedPlanNote(rejection: string): string {
  const json = rejection.slice(rejection.indexOf("{"));
  try {
    const body = JSON.parse(json) as {
      failed_step?: number;
      reason?: string;
      expected?: string;
      offered?: { option_ids?: string[] };
    };
    if (body.reason !== "option unavailable" || body.failed_step === undefined) return "";
    return `\nUI-staged invalid plan: plan step ${body.failed_step} not offered by the engine (${body.expected ?? "?"}): offered ${(body.offered?.option_ids ?? []).join(", ")}`;
  } catch {
    return "";
  }
}

async function uiVersion(page: Page): Promise<number> {
  const text = await page
    .getByTestId("game-version")
    .innerText()
    .catch(() => "");
  return Number(text.replace(/^v/, "")) || 0;
}

async function visibleErrors(page: Page): Promise<string[]> {
  const texts: string[] = [];
  for (const id of ERROR_BANNERS) {
    for (const el of await page.getByTestId(id).all()) {
      if (await el.isVisible()) texts.push((await el.innerText()).trim());
    }
  }
  return texts.filter(Boolean);
}

export async function randomUiPlaythrough(
  browser: Browser,
  request: APIRequestContext,
  options: PlaythroughOptions,
): Promise<PlaythroughReport> {
  const log = options.log ?? (() => {});
  tradeBoost = basePreset(options.startPreset) === "notes";
  const timingOn = process.env.TI4_SMOKE_TIMING === "1";
  const rng = mulberry32(options.clickSeed);
  // Offers of each (seat, subtype) so far; the optional technology windows decline after a few.
  const techOffers = new Map<string, number>();
  const { gameId, players } = await createStartedGame(
    request,
    options.playerCount,
    options.gameSeed,
    options.startPreset,
    options.cardSet,
    options.mapTemplate,
  );
  log(
    `game ${gameId} seed=${options.gameSeed} clickSeed=${options.clickSeed} preset=${options.startPreset ?? "none"} cards=${options.cardSet ?? "default"} map=${options.mapTemplate ?? "default"}`,
  );

  const flags = exerciseFlagsFromEnv();
  const shotCap = shotCapFromEnv();
  const exerciseRng = mulberry32((options.clickSeed ^ 0x9e3779b9) >>> 0);
  // The recap toggle is per browser: one random non-host seat turns it on.
  const recapSeat = flags.recap && players.length > 1 ? 1 + Math.floor(exerciseRng() * (players.length - 1)) : -1;
  const findings: string[] = [];
  const finding = (text: string) => {
    if (!findings.includes(text)) findings.push(text);
    log(`  FINDING: ${text}`);
  };
  let redo: RedoExercise | undefined;
  const browserErrors: string[] = [];
  const pages: Page[] = [];
  const pageLoads: NonNullable<PlaythroughReport["pageLoads"]> = [];
  // The last websocket frames each seat received, so a stuck decision can show whether the tab
  // was ever sent it, or had it cleared afterwards.
  const wsFrames: string[][] = [];
  for (const [index, player] of players.entries()) {
    // A host short of resources can make the browser refuse a seat's page modules; open it again.
    for (let attempt = 1; ; attempt++) {
      const seatErrorsFrom = browserErrors.length;
      const context = await browser.newContext();
      // The harness clicks the bar itself; the client's lone-case auto-submit would race it.
      // TI4_SMOKE_AUTO_LONE=1 leaves the default (on) to exercise that path.
      if (!process.env.TI4_SMOKE_AUTO_LONE)
        await context.addInitScript(() => {
          try {
            localStorage.setItem("player_auto_submit_lone", "false");
          } catch {
            // storage unavailable
          }
        });
      if (flags.recap) await context.addInitScript(recapInitScript, index === recapSeat);
      const page = await context.newPage();
      page.on("pageerror", (err) =>
        browserErrors.push(`[seat ${index + 1}] ${err.message}`),
      );
      const frames: string[] = (wsFrames[index] = []);
      page.on("websocket", (ws) => {
        ws.on("framesent", ({ payload }) => {
          try {
            const m = JSON.parse(String(payload));
            if (m.type === "ping") return;
            frames.push(
              `SENT ${m.type} nonce=${String(m.nonce ?? "").slice(0, 6)} option=${m.option_id ?? ""} v${m.expected_version ?? "?"}`,
            );
          } catch {
            frames.push("SENT unparsed frame");
          }
          if (frames.length > 40) frames.shift();
        });
        ws.on("framereceived", ({ payload }) => {
          let line: string;
          try {
            const m = JSON.parse(String(payload));
            const pending = m.pending_choice?.nonce ?? m.nonce ?? null;
            line = `${m.type} v${m.game_version ?? m.entry?.version ?? "?"}${pending ? ` nonce=${String(pending).slice(0, 6)}` : ""}${m.type === "turn_status" ? ` ${m.status?.kind}` : ""}${m.type === "state_update" ? ` pending=${m.pending_choice ? "yes" : "no"}` : ""}`;
          } catch {
            line = "unparsed frame";
          }
          frames.push(line);
          if (frames.length > 40) frames.shift();
        });
      });
      page.on("console", (msg) => {
        if (process.env.TI4_PREPDBG_FILE && msg.text().startsWith("PREPDBG ")) {
          try {
            appendFileSync(process.env.TI4_PREPDBG_FILE, `S${index + 1} ${msg.text().slice(8)}\n`);
          } catch { /* ignore */ }
        }
        if (msg.type() !== "error") return;
        const url = msg.location().url ?? "";
        // The turn redo exercise records its own HTTP failures as findings.
        if (url.includes("/turn-redo")) return;
        if (url.includes("favicon")) return;
        // A refused heartbeat or lobby poll is retried by the client on its next tick.
        if (tolerableConsoleError(msg.text(), url)) {
          log(`  [seat ${index + 1}] tolerated: ${msg.text()} (${url})`);
          return;
        }
        browserErrors.push(`[seat ${index + 1}] console: ${msg.text()}${url ? ` (${url})` : ""}`);
      });
      // TI4_SMOKE_TIMING=1: what loading this seat's page cost (dev server: hundreds of module requests).
      const loadStats = { requests: 0, scripts: 0, done: false };
      const loadStarted = Date.now();
      if (timingOn)
        page.on("request", (request) => {
          if (loadStats.done) return;
          loadStats.requests++;
          if (request.resourceType() === "script") loadStats.scripts++;
        });
      if (timingOn)
        page.on("requestfinished", (request) => {
          // The server's share of a committing click: request sent until the response was complete.
          if (!request.url().includes("/batches")) return;
          const { responseEnd } = request.timing();
          if (responseEnd < 0) return;
          report.timing ??= {};
          const slot = (report.timing.batchHttp ??= { ms: 0, calls: 0 });
          slot.ms += responseEnd;
          slot.calls++;
        });
      // The console only reports a status code; keep the server's reason for failed API calls.
      page.on("response", async (response) => {
        if (!response.url().includes("/api/")) return;
        if (response.status() < 400) {
          // A batch the server paused at a reaction window is progress, not a rejection.
          if (response.url().includes("/batches")) {
            const paused = pausedBatch(await response.text().catch(() => ""));
            if (paused) {
              report.pausedBatches += 1;
              log(
                `  [seat ${index + 1}] batch paused after ${paused.applied} steps, ${paused.remaining} left, waiting on ${paused.waiting}`,
              );
            }
          }
          return;
        }
        const body = await response.text().catch(() => "");
        if (response.url().includes("/turn-redo")) {
          if (redo) redo.noteHttp(index, response.request().method(), response.status(), body);
          else browserErrors.push(`[seat ${index + 1}] turn-redo ${response.status()}: ${body.slice(0, 300)}`);
          return;
        }
        const line = `[seat ${index + 1}] ${response.request().method()} ${new URL(response.url()).pathname} ${response.status()}: ${body.slice(0, 500)}`;
        browserErrors.push(line);
        log(`  ${line}`);
      });
      await openPlayerGame(page, gameId, player.session);
      try {
        await expect(page.getByTestId("turn-status-bar")).toBeVisible();
      } catch (err) {
        const seatErrors = browserErrors.slice(seatErrorsFrom);
        if (attempt < 3 && startupRetryable(seatErrors)) {
          log(
            `  [seat ${index + 1}] the browser refused ${seatErrors.length} page module(s) (ERR_INSUFFICIENT_RESOURCES); opening the seat again (attempt ${attempt + 1})`,
          );
          await context.close().catch(() => {});
          browserErrors.splice(seatErrorsFrom);
          await new Promise((resolve) => setTimeout(resolve, 2_000));
          continue;
        }
        // Say what the tab showed instead (an error page, a crashed app) rather than only "not found".
        const text = await page.locator("body").innerText().catch(() => "");
        await page.screenshot({ path: `test-results/smoke-open-failure-${gameId}-seat${index + 1}.png` }).catch(() => {});
        throw new Error(
          `seat ${index + 1} never showed the status bar (game ${gameId}). Page text: ${text.slice(0, 600)}\nBrowser errors so far: ${browserErrors.join(" | ").slice(0, 800)}\n${err instanceof Error ? err.message.split("\n")[0] : err}`,
        );
      }
      loadStats.done = true;
      if (timingOn)
        pageLoads.push({ seat: index + 1, ms: Date.now() - loadStarted, requests: loadStats.requests, scripts: loadStats.scripts });
      pages.push(page);
      break;
    }
  }

  const report: PlaythroughReport = {
    gameId,
    startPreset: options.startPreset ?? null,
    cardSet: options.cardSet ?? null,
    decisions: 0,
    clicks: 0,
    finished: false,
    roundStarts: {},
    finalStatus: null,
    subtypes: {},
    rejections: [],
    pausedBatches: 0,
    turnMenuDecisions: 0,
    barDecisions: 0,
    barControls: {},
    turnMenuFallbacks: [],
    reactionDecisions: 0,
    reactionTextProblems: [],
    expectMissing: [],
    exercises: {},
    prep: emptyPrepReport(),
    findings,
    pageLoads: pageLoads.length ? pageLoads : undefined,
    shots: { taken: 0, cap: shotCap, names: [] },
  };

  const timed = async <T>(key: string, work: () => Promise<T>): Promise<T> => {
    if (!timingOn) return work();
    const started = Date.now();
    try {
      return await work();
    } finally {
      report.timing ??= {};
      const slot = (report.timing[key] ??= { ms: 0, calls: 0 });
      slot.ms += Date.now() - started;
      slot.calls++;
    }
  };

  const trace = (file: string, data: unknown, append = false) => {
    if (!options.traceDir) return;
    const path = join(options.traceDir, file);
    const text = append
      ? `${JSON.stringify(data)}\n`
      : JSON.stringify(data, null, 2);
    if (append) appendFileSync(path, text);
    else writeFileSync(path, text);
  };
  if (options.traceDir) mkdirSync(options.traceDir, { recursive: true });
  trace("game.json", {
    gameId,
    players: players.map((p) => p.id),
    options: { ...options, log: undefined },
  });
  let finishExercisesHook: () => Promise<void> = async () => {};
  const writeFinal = async () => {
    if (!options.traceDir) return;
    await finishExercisesHook();
    trace("report.json", report);
    const snapshot = await gameSnapshot(
      request,
      gameId,
      players[0].session,
    ).catch(() => null);
    if (snapshot) trace("final-snapshot.json", snapshot);
  };

  const shotsTaken = new Set<string>();
  const shot = async (page: Page, key: string, name: string) => {
    if (!options.traceDir || shotsTaken.size >= shotCap || shotsTaken.has(key)) return;
    shotsTaken.add(key);
    report.shots.taken = shotsTaken.size;
    report.shots.names.push(name);
    mkdirSync(join(options.traceDir, "shots"), { recursive: true });
    await page
      .screenshot({ path: join(options.traceDir, "shots", `${name}.png`) })
      .catch(() => {});
  };

  const shotSubtypes = new Set(
    (process.env.TI4_SMOKE_SHOT_SUBTYPES ?? "").split(",").map((x) => x.trim()).filter(Boolean),
  );
  const subtypeShots = new Set<string>();
  const shotSubtypesDone = new Set<string>();
  const subtypeShot = async (page: Page, name: string) => {
    const dir = process.env.TI4_SMOKE_SHOT_DIR ?? join(options.traceDir ?? "test-results", "decision-shots");
    mkdirSync(dir, { recursive: true });
    const original = page.viewportSize();
    for (const [label, size] of [
      ["desktop", { width: 1440, height: 900 }],
      ["phone", { width: 390, height: 844 }],
    ] as const) {
      await page.setViewportSize(size);
      await page.waitForTimeout(500);
      await page.screenshot({ path: join(dir, `${name}-${label}.png`), fullPage: true }).catch(() => {});
    }
    if (original) await page.setViewportSize(original);
    log(`  SHOT ${name}`);
  };

  const tour = flags.tour ? new UiTour(finding, shot) : undefined;
  if (tour) report.exercises.tour = { faction: "pending", unit: "pending", notes: tour.notes };
  if (flags.redo) {
    redo = new RedoExercise({
      request,
      gameId,
      sessions: players.map((p) => p.session),
      pages,
      seatIds: players.map((p) => p.id),
      rng: exerciseRng,
      log,
      finding,
      version: async () => (await gameSnapshot(request, gameId, players[0].session)).game_version,
      shot,
    });
    report.exercises.redo = redo.report;
  }
  const recapSamples = async () => {
    if (recapSeat < 0) return;
    const seen = await readRecapToasts(pages[recapSeat]);
    const on = await pages[recapSeat]
      .getByTestId("turn-recap-btn")
      .first()
      .getAttribute("aria-pressed")
      .catch(() => null);
    report.exercises.recap = {
      seat: recapSeat + 1,
      toggleOn: on === "true",
      toastsSeen: seen.length,
      samples: seen.slice(0, 5),
      toastKinds: await readToastKinds(pages[recapSeat]),
    };
  };
  const finishExercises = async () => {
    if (tour) {
      report.exercises.tour!.faction = tour.faction;
      report.exercises.tour!.unit = tour.unit;
      if (!tour.factionDone) finding("UI tour: the Faction card was never reached (no calm turn-menu moment)");
      if (!tour.unitDone) report.exercises.tour!.notes.push("no unit card on screen in this run (no production builder opened)");
    }
    await recapSamples();
    if (flags.recap)
      report.exercises.toastKindsBySeat = await Promise.all(pages.map((pg) => readToastKinds(pg)));
    const rc = report.exercises.recap;
    if (rc) {
      if (!rc.toggleOn) finding(`Recap: the toggle was not on for seat ${rc.seat} although it was enabled`);
      else if (rc.toastsSeen === 0 && report.decisions > 60)
        finding(`Recap: seat ${rc.seat} had the recap on but saw no recap toast in ${report.decisions} decisions`);
    }
    if (redo && redo.report.outcome === "not attempted")
      redo.report.notes.push("no calm moment after enough completed turns before the run ended");
    report.shots.taken = shotsTaken.size;
  };

  finishExercisesHook = () => finishExercises().catch(() => {});
  const prepConfig = prepConfigFromEnv();
  const prepRng = mulberry32((options.clickSeed ^ 0x5bd1e995) >>> 0);
  const prep = prepConfig.enabled
    ? new SecondaryPrepExercise({
        pages,
        rng: prepRng,
        config: prepConfig,
        log,
        finding,
        shot,
        collect: (page) => collectCandidates(page),
        pickOne: (cands, clicks) =>
          pick(cands as Candidate[], clicks, prepRng, "random", new Map<string, number>()),
        activeInfo: async (seat) => {
          const snap = await gameSnapshot(request, gameId, players[seat].session);
          const active = snap.view.active_player;
          return `server view.active_player=${active === players[seat].id ? "this seat" : active ? `seat ${players.findIndex((p) => p.id === active) + 1}` : "none"}`;
        },
        eventDetails: async (seat) => {
          const snap = await gameSnapshot(request, gameId, players[seat].session);
          return (snap.events ?? []).map((event) => event.detail ?? "");
        },
        label: `game seed ${options.gameSeed}, click seed ${options.clickSeed}`,
      })
    : undefined;
  if (prep) report.prep = prep.report;

  let traceWritten = false;
  const fail = async (
    page: Page | undefined,
    message: string,
  ): Promise<never> => {
    traceWritten = true;
    const status = await gameSnapshot(
      request,
      gameId,
      players[0].session,
    ).catch(() => null);
    const seatIndex = page ? pages.indexOf(page) : -1;
    const frames =
      seatIndex >= 0
        ? `\nws frames seat ${seatIndex + 1} (newest last):\n${wsFrames[seatIndex].join("\n")}`
        : "";
    const detail = `${message}\nreport: ${JSON.stringify({ ...report, finalStatus: status?.turn_status })}${frames}`;
    if (page)
      await page
        .screenshot({ path: `test-results/smoke-failure-${gameId}.png` })
        .catch(() => {});
    trace("failure.txt", detail);
    await writeFinal();
    throw new Error(detail);
  };

  try {
    let noChoicePolls = 0;
    while (report.decisions < options.maxDecisions) {
      // Through `fail` so the trace (failure.txt, report.json) is written for browser errors too.
      if (browserErrors.length)
        await fail(
          undefined,
          `browser errors during playthrough:\n${browserErrors.join("\n")}`,
        );
      const state = await timed("status", () =>
        gameSnapshot(request, gameId, players[0].session),
      );
      const status = state.turn_status;
      report.finalStatus = status;
      if (status.kind === "game_over") {
        report.finished = true;
        break;
      }
      if (!(status.round in report.roundStarts)) {
        report.roundStarts[status.round] = report.decisions;
        log(`round ${status.round} reached after ${report.decisions} decisions`);
      }
      if (
        options.stopAtRound !== undefined &&
        status.round >= options.stopAtRound
      )
        break;
      if (redo && !redo.over) {
        // Between decisions, no click in flight: the turn redo exercise may take over for a moment.
        let calm: { seat: string; prompt: string | undefined; phase: string } | null = null;
        if (redo.wantsCalm() && status.kind === "waiting_for_decision") {
          const seatIdx = players.findIndex((p) => p.id === status.seat);
          if (seatIdx >= 0) {
            const snap = await gameSnapshot(request, gameId, players[seatIdx].session);
            calm = { seat: status.seat, prompt: snap.pending_choice?.choice.prompt, phase: status.phase };
          }
        }
        if ((await redo.step(calm, report.decisions)) === "retry") continue;
      }
      if (redo?.stuck) break;
      if (
        status.kind !== "waiting_for_decision" &&
        status.kind !== "waiting_for_reactions"
      ) {
        // Nothing to click; the server should move on by itself. A batch commit replays the whole
        // game into a replacement session before it is published; until then readers see the
        // stopped session, which has no pending decision and an unchanged version. On a debug
        // build late in a game that takes longer than 10 s (run 4, round 6, 916 decisions).
        const moved = await expect
          .poll(
            async () =>
              (await gameSnapshot(request, gameId, players[0].session))
                .game_version,
            {
              timeout: 45_000,
            },
          )
          .toBeGreaterThan(state.game_version)
          .then(() => true)
          .catch(() => false);
        if (!moved) {
          // The game may have ended (objective decks exhausted) after the snapshot above was taken.
          const latest = await gameSnapshot(request, gameId, players[0].session);
          if (latest.turn_status.kind === "game_over") continue;
          await fail(
            undefined,
            `game idle without a decision: ${JSON.stringify(status)}`,
          );
        }
        continue;
      }

      // A reaction window names no seat in the public status (who holds a card is private), so
      // the asked seat is the one whose own snapshot carries the pending choice.
      let actorIndex = -1;
      let actorState = state;
      if (status.kind === "waiting_for_decision") {
        actorIndex = players.findIndex((p) => p.id === status.seat);
        if (actorIndex < 0)
          await fail(undefined, `decision for unknown seat ${status.seat}`);
        actorState = await timed("actor", () =>
          gameSnapshot(request, gameId, players[actorIndex].session),
        );
      } else {
        for (let i = 0; i < players.length; i++) {
          const snapshot = await gameSnapshot(request, gameId, players[i].session);
          if (snapshot.pending_choice?.choice) {
            actorIndex = i;
            actorState = snapshot;
            break;
          }
        }
      }
      const page = pages[Math.max(actorIndex, 0)];
      const choice = actorState.pending_choice?.choice;
      // The status was read before the offer moved on (e.g. to the secondary of a strategy card); re-poll.
      if (!choice) {
        if (++noChoicePolls > 40)
          await fail(
            undefined,
            `${status.kind === "waiting_for_decision" ? `seat ${status.seat}` : "no seat"} is waiting but has no pending choice: ${JSON.stringify(status)}`,
          );
        await new Promise((resolve) => setTimeout(resolve, 100));
        continue;
      }
      noChoicePolls = 0;
      const subtype =
        choice?.context?.subtype ??
        `prompt:${choice?.prompt.slice(0, 40) ?? "none"}`;
      const before = actorState.game_version;
      const hexWeights =
        options.policy === "steer" && subtype === "activate_system"
          ? activationWeights(actorState.view.board, players[actorIndex].id, basePreset(options.startPreset) === "explore")
          : new Map<string, number>();
      report.subtypes[subtype] = (report.subtypes[subtype] ?? 0) + 1;
      const offerKey = `${actorIndex}:${subtype}`;
      const techOffersSoFar = techOffers.get(offerKey) ?? 0;
      techOffers.set(offerKey, techOffersSoFar + 1);
      if (status.round <= 3)
        await shot(page, `round-${status.round}`, `round-${status.round}-d${report.decisions}-${subtype}`.slice(0, 120));
      trace(
        "trace.jsonl",
        {
          decision: report.decisions,
          version: before,
          round: status.round,
          phase: status.phase,
          seat: actorIndex + 1,
          player: players[actorIndex].id,
          faction: actorState.view.players?.find(
            (p) => p.id === players[actorIndex].id,
          )?.faction,
          subtype,
          source: choice?.context?.source ?? null,
          prompt: choice?.prompt,
          options: choice?.options.map((o) => ({
            id: o.id,
            kind: o.kind,
            label: o.label,
          })),
        },
        true,
      );

      // Let the actor's tab catch up with the server before reading its controls.
      await timed("ui", () =>
        expect
          .poll(() => uiVersion(page), { timeout: 10_000 })
          .toBeGreaterThanOrEqual(before)
          .catch(() =>
            fail(page, `seat ${actorIndex + 1} UI never reached v${before}`),
          ),
      );

      // TI4_SMOKE_SHOT_SUBTYPES=a,b: save the deciding seat's real view of each listed subtype the
      // first time it is shown (before answering) at desktop and phone size, and again after the answer.
      const wantShot = shotSubtypes.has(subtype) && !subtypeShots.has(subtype);
      if (wantShot) {
        subtypeShots.add(subtype);
        await page.waitForTimeout(600);
        await subtypeShot(page, `${subtype}-seat${actorIndex + 1}-before`);
      }
      const isTurnMenu =
        choice.prompt === "action phase" || subtype === "end_turn";
      if (isTurnMenu) report.turnMenuDecisions++;
      let barControl: string | null = null;
      let progressed = false;
      let emptyPolls = 0;
      const isReaction = isReactionSubtype(subtype);
      if (isReaction) report.reactionDecisions++;
      let reactionChecked = !isReaction;
      if (tour && !tour.finished) {
        const productionBuilder =
          (await page.getByTestId("production-builder-drawer").count().catch(() => 0)) > 0;
        await timed("tour", () => tour.step(page, { isTurnMenu, productionBuilder }));
      }
      if (prep) {
        // Secondary pre-planning: offer the waiting seats, and let a saved plan answer this decision.
        const handled = await timed("prep", async () => {
          await prep.offer(actorIndex, status.round, report.decisions, choice.prompt);
          return prep.handle(
            actorIndex,
            choice as unknown as { details?: Record<string, unknown>; prompt?: string },
            status.round,
            before,
            () => uiVersion(page),
          );
        });
        if (handled === "done") progressed = true;
      }
      for (let clicks = 0; !progressed && clicks < options.maxClicksPerDecision;) {
        // Progress is read from the actor's tab (free) rather than the API; it follows the server
        // over the websocket. A late-landing commit is caught here before another click.
        if (clicks > 0 && (await uiVersion(page)) > before) {
          progressed = true;
          break;
        }
        if (!reactionChecked) {
          const shown = await page
            .getByTestId("reaction-status-bar")
            .first()
            .textContent({ timeout: 500 })
            .catch(() => null);
          if (shown !== null) {
            reactionChecked = true;
            const problem = reactionTextProblem(shown);
            if (problem) report.reactionTextProblems.push(`${subtype}: ${problem}`);
          }
        }
        if (options.traceDir && shotsTaken.size < shotCap) {
          const shown = await page
            .evaluate(
              (ids) => ids.filter((id) => document.querySelector(`[data-testid="${id}"]`) !== null),
              SHOT_TARGETS,
            )
            .catch(() => [] as string[]);
          for (const id of shown)
            await shot(page, id, `${id}-d${report.decisions}-${subtype}`.slice(0, 120));
        }
        const candidates = await timed("scan", () => collectCandidates(page));
        if (!candidates.length) {
          // The UI can take a moment to mount the workflow for a fresh offer. Late in a game the
          // UI may also be finishing its own multi-step plan (a production builder submits one
          // decision per unit and each commit replays the whole game on a debug build, seconds
          // apiece), so the offer shows no control until the plan has landed (run 1c, decision
          // #1153: 3 s was not enough). The wait is long; the version check below still ends it
          // as soon as the server moves on.
          if (++emptyPolls > 250) {
            await fail(
              page,
              `no actionable control for ${subtype} (seat ${actorIndex + 1}); options: ${JSON.stringify(choice?.options.map((o) => o.id))}`,
            );
          }
          await page.waitForTimeout(100);
          if (
            serverProgressed(
              await gameSnapshot(request, gameId, players[0].session),
              before,
            )
          ) {
            progressed = true;
            break;
          }
          continue;
        }
        const chosen = pick(
          preferTechWindowUse(
            subtype,
            choice.prompt,
            candidates,
            techOffersSoFar,
            process.env.TI4_SMOKE_TECH_FULL !== "1",
          ),
          clicks,
          rng,
          options.policy ?? "random",
          hexWeights,
        );
        log(
          `#${report.decisions} ${subtype} seat${actorIndex + 1} click ${chosen.desc}`,
        );
        await timed("click", () =>
          page
            .locator(`[data-smoke-idx="${chosen.idx}"]`)
            .click({ timeout: 2_000 })
            .catch((err: Error) =>
              log(`  click failed: ${err.message.split("\n")[0]}`),
            ),
        );
        clicks++;
        report.clicks++;
        if (isTurnMenu && isBarControl(chosen.desc) && chosen.commit)
          barControl = chosen.desc
            .split(" | ")[0]
            .replace(/^(turn-bar-open)-.*/, "$1")
            .replace(/^(turn-bar-item)-.*/, "$1");

        if (chosen.commit) {
          // Wait for the tab to show the new version; fall back to one server check in case the
          // tab's websocket lagged.
          progressed = await timed("commit", () =>
            expect
              .poll(() => uiVersion(page), { timeout: 3_000, intervals: [50] })
              .toBeGreaterThan(before)
              .then(() => true)
              .catch(async () =>
                // gameSnapshot retries once with a longer timeout; if that fails too the run ends with
                // failure.txt, report.json and a best-effort final snapshot (not a bare timeout).
                gameSnapshot(request, gameId, players[0].session).then(
                  (latest) => serverProgressed(latest, before),
                  (err: Error) =>
                    fail(
                      page,
                      `snapshot request failed after ${subtype} (decision #${report.decisions}): ${err.message.split("\n")[0]}`,
                    ),
                ),
              ),
          );
        } else {
          // Staging clicks rarely advance the server: let React settle, then read the tab once.
          await timed("settle", async () => {
            await page.waitForTimeout(40);
            progressed = (await uiVersion(page)) > before;
          });
        }
        if (progressed) break;
        // A rejected batch is rejected again if re-sent, so stop at the first one.
        const batchRejection = browserErrors.find((e) =>
          /\/batches \d{3}:/.test(e),
        );
        if (batchRejection) {
          report.rejections.push(`${subtype}: ${batchRejection}`);
          await fail(page, `batch rejected: ${batchRejection}${stagedPlanNote(batchRejection)}`);
        }
        const errors = await visibleErrors(page);
        for (const error of errors) {
          const entry = `${subtype}: ${error}`;
          if (!report.rejections.includes(entry)) {
            report.rejections.push(entry);
            log(`  rejected: ${error}`);
          }
        }
      }
      if (!progressed) {
        progressed =
          (await gameSnapshot(request, gameId, players[0].session)).game_version >
          before;
      }
      if (!progressed) {
        await fail(
          page,
          `${subtype} did not advance after ${options.maxClicksPerDecision} clicks (seat ${actorIndex + 1}); options: ${JSON.stringify(choice?.options.map((o) => o.id))}`,
        );
      }
      if (wantShot) {
        await page.waitForTimeout(800);
        await subtypeShot(page, `${subtype}-seat${actorIndex + 1}-after`);
        shotSubtypesDone.add(subtype);
        if (process.env.TI4_SMOKE_SHOT_STOP && shotSubtypesDone.size >= shotSubtypes.size) break;
      }
      if (isTurnMenu) {
        if (barControl) {
          report.barDecisions++;
          report.barControls[barControl] =
            (report.barControls[barControl] ?? 0) + 1;
        } else {
          report.turnMenuFallbacks.push(
            `${subtype}: ${JSON.stringify(choice.options.map((o) => o.id))}`,
          );
        }
      }
      if (redo)
        redo.noteResolved(players[actorIndex].id, subtype, choice.prompt, barControl ?? "");
      report.decisions++;
    }

  } catch (err) {
    // Anything not raised through `fail` (a snapshot or click error) still leaves the evidence.
    if (!traceWritten) {
      traceWritten = true;
      const message = err instanceof Error ? err.message : String(err);
      trace(
        "failure.txt",
        `unexpected error: ${message.split("\n")[0]}\nreport: ${JSON.stringify(report)}`,
      );
      await writeFinal();
    }
    throw err;
  }
  report.finalStatus = (
    await gameSnapshot(request, gameId, players[0].session)
  ).turn_status;
  if (report.finished) {
    // The game ended: every seat's tab must say so (the banner is the UI's whole game-over
    // screen) without a reload. Before the client handled the game_over push every tab stayed on
    // the last phase banner.
    const shows = (page: Page, timeout: number) =>
      expect
        .poll(
          async () =>
            (await page
              .getByTestId("turn-status-banner")
              .textContent()
              .catch(() => "")) ?? "",
          { timeout },
        )
        .toMatch(/Game Over/)
        .then(() => true)
        .catch(() => false);
    for (const [index, page] of pages.entries()) {
      if (!(await shows(page, 10_000)))
        await fail(page, `seat ${index + 1} never showed the game-over banner`);
    }
  }
  trace("browser-errors.json", browserErrors);
  report.expectMissing = missingExpected(
    // "game_over" counts as a decision subtype for expectations, so a run can assert it ended.
    report.finished ? { ...report.subtypes, game_over: 1 } : report.subtypes,
    options.expect ?? [],
  );
  await writeFinal();
  expect(browserErrors, "browser errors during playthrough").toEqual([]);
  if (report.expectMissing.length)
    await fail(
      undefined,
      `expected decisions never offered: ${report.expectMissing.join("; ")}`,
    );
  return report;
}
