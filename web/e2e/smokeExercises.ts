import type { APIRequestContext, Page } from "@playwright/test";

/**
 * Optional UI exercises for the smoke playthrough. None of them is part of a plain run:
 *
 * - TI4_SMOKE_UI_TOUR=1   open one unit card and the Faction card at a calm moment, check them.
 * - TI4_SMOKE_RECAP=1     one random non-host seat turns the "Recap" toggle on; the toasts are counted.
 * - TI4_SMOKE_REDO=1      one guarded "Redo my last turn" round trip (rewind, new turn, auto-play,
 *                         then restore or keep).
 * - TI4_SMOKE_REDO_MIDTURN=1  with TI4_SMOKE_REDO: redo the turn the seat is still playing (at its
 *                         "end your turn" question), and log every error text any tab shows meanwhile.
 *
 * Every exercise records what it saw in the report (`exercises`, `findings`) and never fails the
 * run by itself: a problem becomes a finding and the run continues (or ends cleanly).
 */

const backend = `http://127.0.0.1:${process.env.TI4_E2E_BACKEND_PORT ?? "8080"}`;

export interface ExerciseFlags {
  tour: boolean;
  recap: boolean;
  redo: boolean;
}

const on = (name: string) => {
  const v = process.env[name];
  return !!v && v !== "0" && v.toLowerCase() !== "false";
};

export function exerciseFlagsFromEnv(): ExerciseFlags {
  return {
    tour: on("TI4_SMOKE_UI_TOUR"),
    recap: on("TI4_SMOKE_RECAP"),
    redo: on("TI4_SMOKE_REDO"),
  };
}

/** Screenshot cap: a number, or "all" (= 20, the historic maximum). Default 20. */
export function shotCapFromEnv(value = process.env.TI4_SMOKE_SHOT_CAP): number {
  if (!value || value.toLowerCase() === "all") return 20;
  const n = Number.parseInt(value, 10);
  return Number.isFinite(n) && n >= 0 ? n : 20;
}

export interface ExerciseReport {
  /** Recap runs: corner toasts by kind that each seat's tab saw (index = seat - 1). */
  toastKindsBySeat?: Record<string, number>[];
  tour?: {
    faction: string;
    unit: string;
    notes: string[];
  };
  recap?: {
    seat: number;
    toggleOn: boolean;
    toastsSeen: number;
    samples: string[];
    /** Corner toasts the seat saw, by kind (recap, action, auto-resolve). */
    toastKinds: Record<string, number>;
  };
  redo?: {
    outcome: string;
    seat?: number;
    choice?: "restore" | "keep";
    stopKind?: string;
    keptDecisions?: number;
    versions: string[];
    notes: string[];
  };
}

// ---------------------------------------------------------------------------------------------
// Recap toasts
// ---------------------------------------------------------------------------------------------

/** Init script for the recap seat: turn the toggle on and count the recap toasts that appear. */
export function recapInitScript(enable: boolean) {
  try {
    if (enable) localStorage.setItem("player_turn_recap", "true");
  } catch {
    // storage unavailable
  }
  const w = window as unknown as { __recapSeen?: string[]; __toastKinds?: Record<string, number> };
  w.__recapSeen = [];
  w.__toastKinds = {};
  const note = (node: Node) => {
    if (!(node instanceof Element)) return;
    const hits = node.matches("[data-toast-kind]")
      ? [node]
      : Array.from(node.querySelectorAll("[data-toast-kind]"));
    for (const el of hits) {
      const kind = el.getAttribute("data-toast-kind") ?? "?";
      (w.__toastKinds as Record<string, number>)[kind] = ((w.__toastKinds as Record<string, number>)[kind] ?? 0) + 1;
      if (kind === "recap") (w.__recapSeen as string[]).push((el.textContent ?? "").trim().replace(/\s+/g, " "));
    }
  };
  const start = () => {
    (w.__toastKinds as Record<string, number>)["_observer"] = 1;
    new MutationObserver((records) => {
      for (const r of records) r.addedNodes.forEach(note);
    }).observe(document.documentElement, { childList: true, subtree: true });
  };
  if (document.documentElement) start();
  else document.addEventListener("DOMContentLoaded", start);
}

export async function readToastKinds(page: Page): Promise<Record<string, number>> {
  return page
    .evaluate(() => (window as unknown as { __toastKinds?: Record<string, number> }).__toastKinds ?? {})
    .catch(() => ({}) as Record<string, number>);
}

export async function readRecapToasts(page: Page): Promise<string[]> {
  return page
    .evaluate(() => (window as unknown as { __recapSeen?: string[] }).__recapSeen ?? [])
    .catch(() => [] as string[]);
}

// ---------------------------------------------------------------------------------------------
// UI tour
// ---------------------------------------------------------------------------------------------

export class UiTour {
  factionDone = false;
  unitDone = false;
  notes: string[] = [];
  faction = "pending";
  unit = "pending";
  constructor(
    private readonly finding: (text: string) => void,
    private readonly shot: (page: Page, key: string, name: string) => Promise<void>,
  ) {}

  get finished() {
    return this.factionDone && this.unitDone;
  }

  /** Open one info card, assert it, press Escape, check it closed. Returns a result string. */
  private async card(page: Page, trigger: string, label: string, expectText: boolean): Promise<string> {
    const button = page.locator(trigger).first();
    await button.scrollIntoViewIfNeeded({ timeout: 2_000 });
    await button.click({ timeout: 3_000 });
    const card = page.locator(".info-card:visible").first();
    await card.waitFor({ state: "visible", timeout: 3_000 });
    await page.waitForTimeout(250); // let positioning settle
    const info = await card.evaluate((el) => {
      const r = el.getBoundingClientRect();
      const style = getComputedStyle(el);
      return {
        text: (el.textContent ?? "").trim(),
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
        w: window.innerWidth,
        h: window.innerHeight,
        clippedX: el.scrollWidth > el.clientWidth + 2 && !/auto|scroll/.test(style.overflowX),
        clippedY: el.scrollHeight > el.clientHeight + 2 && !/auto|scroll/.test(style.overflowY),
        empty: !!el.querySelector('[data-testid$="info-empty"]'),
      };
    });
    const problems: string[] = [];
    if (info.text.length < 20) problems.push(`card text is nearly empty (${info.text.length} chars)`);
    if (expectText && (info.empty || /No information available/.test(info.text)))
      problems.push("card shows the 'No information available' fallback");
    if (info.left < -1 || info.top < -1 || info.right > info.w + 1 || info.bottom > info.h + 1)
      problems.push(
        `card is off screen (box ${Math.round(info.left)},${Math.round(info.top)} to ${Math.round(info.right)},${Math.round(info.bottom)} in ${info.w}x${info.h})`,
      );
    if (info.clippedX || info.clippedY) problems.push("card content is clipped (overflow hidden)");
    await this.shot(page, `tour-${label}`, `tour-${label}-card`);
    await page.keyboard.press("Escape");
    const closed = await page
      .locator(".info-card:visible")
      .waitFor({ state: "detached", timeout: 3_000 })
      .then(() => true)
      .catch(() => false);
    if (!closed) problems.push("card did not close on Escape");
    for (const p of problems) this.finding(`UI tour (${label}): ${p}`);
    return problems.length ? `problems: ${problems.join("; ")}` : `ok (${info.text.length} chars)`;
  }

  /** Called before the first click of a decision, when nothing of it is staged yet. */
  async step(page: Page, ctx: { isTurnMenu: boolean; productionBuilder: boolean }) {
    if (this.finished) return;
    try {
      if (!this.factionDone && ctx.isTurnMenu) {
        this.factionDone = true;
        const trigger = '[data-testid="faction-info-button"]';
        if (await page.locator(trigger).count()) {
          this.faction = await this.card(page, trigger, "faction", true);
        } else {
          this.faction = "no Faction button on screen";
          this.finding("UI tour (faction): no Faction button found on the player sheet");
        }
      } else if (!this.unitDone && ctx.productionBuilder) {
        const trigger = 'button[data-testid^="unit-info-"], button[data-testid="mech-info"]';
        if (await page.locator(trigger).count()) {
          this.unitDone = true;
          this.unit = await this.card(page, trigger, "unit", true);
        }
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message.split("\n")[0] : String(err);
      this.finding(`UI tour: ${msg}`);
      this.notes.push(msg);
      // Make sure no card stays open over the next decision.
      await page.keyboard.press("Escape").catch(() => {});
      if (!this.factionDone) this.factionDone = true;
      else this.unitDone = true;
    }
  }
}

// ---------------------------------------------------------------------------------------------
// Turn redo
// ---------------------------------------------------------------------------------------------

interface RedoStatusJson {
  seat: string;
  stage: "new_turn" | "auto_played";
  turn_complete: boolean;
  can_control: boolean;
  outcome: { kept: number; tail_total: number; stop: { kind: string; seat?: string } } | null;
}

type Phase = "idle" | "requested" | "new_turn" | "waiting_handoff" | "done" | "stuck";

export interface RedoDeps {
  request: APIRequestContext;
  gameId: string;
  sessions: string[];
  pages: Page[];
  seatIds: string[];
  rng: () => number;
  log: (line: string) => void;
  finding: (text: string) => void;
  version: () => Promise<number>;
  shot: (page: Page, key: string, name: string) => Promise<void>;
}

/** TI4_SMOKE_REDO_MIDTURN=1: redo the turn the seat is still playing (at "end your turn"). */
const REDO_MIDTURN = on("TI4_SMOKE_REDO_MIDTURN");
const REQUEST_TIMEOUT_MS = 30_000;
const NEW_TURN_TIMEOUT_MS = 300_000;
const HANDOFF_TIMEOUT_MS = 150_000;
const FINISH_TIMEOUT_MS = 150_000;

export class RedoExercise {
  phase: Phase = "idle";
  readonly report: NonNullable<ExerciseReport["redo"]> = { outcome: "not attempted", versions: [], notes: [] };
  private completed = new Map<string, number>();
  private order: string[] = [];
  private seatIndex = -1;
  private since = 0;
  private triggerAfter: number;
  private replayClicked = false;
  private decisionsInNewTurn = 0;

  constructor(private readonly d: RedoDeps) {
    // After this many completed turns (counted over all seats) at a calm moment.
    this.triggerAfter = 2 + Math.floor(d.rng() * 4);
  }

  /**
   * The seat whose last turn to redo: one that finished a turn which another seat's turn followed
   * (so the auto-play has a recorded tail to replay), once enough turns were completed.
   */
  private pickSeat(): string | null {
    if (this.order.length < this.triggerAfter) return null;
    if (REDO_MIDTURN) return "any";
    const last = this.order[this.order.length - 1];
    for (let i = this.order.length - 2; i >= 0; i--) if (this.order[i] !== last) return this.order[i];
    return null;
  }

  /** Whether the next `step` needs the pending prompt (cheap checks first, so the caller reads it only then). */
  wantsCalm(): boolean {
    return this.phase === "idle" && this.pickSeat() !== null;
  }

  get active() {
    return this.phase !== "done" && this.phase !== "idle" && this.phase !== "stuck";
  }
  get over() {
    return this.phase === "done" || this.phase === "stuck";
  }
  get stuck() {
    return this.phase === "stuck";
  }

  /** After a decision resolved: remember which seats closed an action-phase turn. */
  noteResolved(seatId: string, subtype: string, prompt: string | undefined, chosen: string) {
    if (this.phase === "new_turn") this.decisionsInNewTurn++;
    const endsTurn =
      subtype === "end_turn" || /^end your turn/i.test(prompt ?? "") || (prompt === "action phase" && /\bpass\b/i.test(chosen));
    if (!endsTurn) return;
    this.completed.set(seatId, (this.completed.get(seatId) ?? 0) + 1);
    this.order.push(seatId);
  }

  private async status(seat = Math.max(this.seatIndex, 0)): Promise<RedoStatusJson | null | undefined> {
    const r = await this.d.request
      .get(`${backend}/api/games/${this.d.gameId}/turn-redo`, {
        headers: { "x-ti4-player-session": this.d.sessions[seat] },
        timeout: 15_000,
      })
      .catch(() => null);
    if (!r) return undefined;
    // Playwright keeps the body in the worker until the response is disposed.
    const body = r.ok() ? ((await r.json().catch(() => null)) as { status: RedoStatusJson | null } | null) : null;
    await r.dispose().catch(() => {});
    return body ? body.status : undefined;
  }

  /** An HTTP error on a /turn-redo route seen by a tab. */
  noteHttp(seat: number, method: string, status: number, body: string) {
    const line = `[seat ${seat + 1}] ${method} turn-redo ${status}: ${body.slice(0, 300)}`;
    this.report.notes.push(line);
    this.d.log(`  redo: ${line}`);
    if (this.phase === "requested") this.httpFailure = line;
    else if (status === 409 && /Game advanced or a decision is in flight/.test(body)) {
      // The tab asked with the version it had when the last decision of the new turn landed; the
      // client treats this refusal as stale and asks again with the new version. Only counted.
      this.report.notes.push("(stale-version 409 above is retried by the client; not a finding)");
    } else {
      // Several tabs (the host and the redoing seat) each fire the auto-play: the losers get a 409/400.
      const kind = `${status} ${body.replace(/\d+/g, "N").slice(0, 80)}`;
      if (!this.httpKinds.has(kind)) {
        this.httpKinds.add(kind);
        this.d.finding(`Turn redo: ${line} (first of its kind; seen from another tab outside the harness's own request)`);
      }
    }
  }
  private httpFailure: string | null = null;
  /** Every error text any tab showed while the redo ran (TI4_SMOKE_REDO_MIDTURN diagnostics). */
  readonly uiErrors: string[] = [];
  private startErrorSampler() {
    const sample = async () => {
      if (this.phase === "done" || this.phase === "stuck") return;
      for (const [i, page] of this.d.pages.entries()) {
        const texts = await page
          .evaluate(() =>
            [...document.querySelectorAll('[data-testid*="error"], .text-danger, [role="alert"]')]
              .filter((el) => (el as HTMLElement).offsetParent !== null)
              .map((el) => `${el.getAttribute("data-testid") ?? el.className}: ${(el.textContent ?? "").trim().slice(0, 300)}`)
              .filter((t) => !t.endsWith(": ")),
          )
          .catch(() => [] as string[]);
        for (const t of texts) {
          const line = `[seat ${i + 1}] ${t}`;
          if (!this.uiErrors.includes(line)) {
            this.uiErrors.push(line);
            this.report.notes.push(`UI ERROR ${line}`);
            this.d.log(`  redo: UI ERROR ${line}`);
          }
        }
      }
      setTimeout(() => void sample(), 250);
    };
    void sample();
  }
  private httpKinds = new Set<string>();

  private async abort(why: string): Promise<void> {
    this.d.finding(`Turn redo aborted: ${why}`);
    this.report.outcome = `aborted: ${why}`;
    const st = await this.status();
    if (st) {
      const page = this.d.pages[this.seatIndex];
      const ok = await this.click(page, "turn-redo-restore");
      const cleared = ok && (await this.waitStatusNull(FINISH_TIMEOUT_MS));
      if (!cleared) {
        this.d.finding("Turn redo: could not leave the redo state after the abort (restore failed); the run ends here");
        this.report.outcome += "; STUCK";
        this.phase = "stuck";
        return;
      }
    }
    this.phase = "done";
  }

  /** Clicks a redo bar button once it is visible and enabled (the tab may still be finishing its own request). */
  private async click(page: Page, testId: string, waitMs = 30_000): Promise<boolean> {
    const el = page.getByTestId(testId).first();
    const end = Date.now() + waitMs;
    let minimized = false;
    while (Date.now() < end) {
      if ((await el.isVisible().catch(() => false)) && (await el.isEnabled().catch(() => false))) {
        try {
          // The seat's own live question is a modal over the bar (by design): minimise it like a
          // player would (Escape) instead of reaching through it.
          if (!minimized && (await page.getByTestId("pending-choice-dialog").isVisible().catch(() => false))) {
            minimized = true;
            await page.keyboard.press("Escape");
            await page.waitForTimeout(300);
          }
          await el.click({ timeout: 4_000 });
          return true;
        } catch (err) {
          // Keep the reason (usually "intercepts pointer events"), then click through the DOM.
          this.report.notes.push(`${testId}: normal click failed: ${(err instanceof Error ? err.message : String(err)).split("\n").slice(0, 12).join(" ").slice(0, 900)}`);
          return el.evaluate((b) => (b as HTMLButtonElement).click()).then(
            () => true,
            () => false,
          );
        }
      }
      await new Promise((r) => setTimeout(r, 300));
    }
    const evidence = await page
      .evaluate((id) => {
        const bar = document.querySelector('[data-testid="turn-redo-bar"]');
        const btn = document.querySelector(`[data-testid="${id}"]`) as HTMLButtonElement | null;
        return `bar=${bar ? `"${(bar.textContent ?? "").slice(0, 120)}" state=${bar.getAttribute("data-state")}` : "absent"} button=${btn ? `present disabled=${btn.disabled}` : "absent"}`;
      }, testId)
      .catch(() => "evidence unavailable");
    this.report.notes.push(`${testId} not clickable: ${evidence}`);
    return false;
  }

  private async waitStatusNull(ms: number): Promise<boolean> {
    const end = Date.now() + ms;
    while (Date.now() < end) {
      if ((await this.status()) === null) return true;
      await new Promise((r) => setTimeout(r, 500));
    }
    return false;
  }

  /**
   * Called at the top of every playthrough iteration, between decisions (no click in flight).
   * `calm` is the seat and prompt of the pending decision, or null when none was read.
   * Returns "go" to carry on with the normal decision handling, "retry" to loop again.
   */
  async step(calm: { seat: string; prompt: string | undefined; phase: string } | null, decisions: number): Promise<"go" | "retry"> {
    try {
      switch (this.phase) {
        case "idle":
          return await this.maybeRequest(calm, decisions);
        case "requested":
          return await this.awaitRewind();
        case "new_turn":
          return await this.watchNewTurn();
        case "waiting_handoff":
          return await this.awaitHandoff();
        default:
          return "go";
      }
    } catch (err) {
      await this.abort(`exercise error: ${err instanceof Error ? err.message.split("\n")[0] : String(err)}`);
      return "retry";
    }
  }

  private async maybeRequest(
    calm: { seat: string; prompt: string | undefined; phase: string } | null,
    decisions: number,
  ): Promise<"go" | "retry"> {
    let seat = this.pickSeat();
    if (!calm || !seat) return "go";
    if (REDO_MIDTURN) {
      // The seat redoes the turn it is still playing, at its "end your turn" question.
      if (calm.phase !== "action" || !/^end your turn/i.test(calm.prompt ?? "")) return "go";
      seat = calm.seat;
    } else if (calm.phase !== "action" || calm.prompt !== "action phase" || calm.seat === seat) return "go";
    // Not in the first few decisions of a quiet moment; and only once per run (phase leaves idle).
    this.seatIndex = this.d.seatIds.indexOf(seat);
    if (this.seatIndex < 0) return "go";
    const page = this.d.pages[this.seatIndex];
    if ((await this.status()) !== null) return "go"; // another redo or an unreadable status: not now
    this.since = Date.now();
    this.report.seat = this.seatIndex + 1;
    const before = await this.d.version();
    this.report.versions.push(`before request v${before} (decision #${decisions})`);
    if (!(await page.getByTestId("turn-redo-btn").first().isVisible().catch(() => false))) {
      const toggle = page.getByTestId("event-log-toggle").first();
      if ((await toggle.getAttribute("aria-expanded").catch(() => "true")) !== "true") {
        await toggle.click({ timeout: 3_000 }).catch(() => {});
      }
    }
    const btn = page.getByTestId("turn-redo-btn").first();
    const enabled = await btn.waitFor({ state: "visible", timeout: 5_000 }).then(() => btn.isEnabled()).catch(() => false);
    if (!enabled) {
      this.phase = "done";
      this.d.finding("Turn redo: the 'Redo my last turn' button was not visible/enabled in the event log at a calm moment");
      this.report.outcome = "aborted: button not available";
      return "retry";
    }
    await this.d.shot(page, "redo-log", `redo-event-log-controls`);
    this.d.log(`  redo: requesting redo of seat ${this.seatIndex + 1}'s last turn at v${before}`);
    this.phase = "requested";
    this.startErrorSampler();
    await btn.click({ timeout: 5_000 });
    return "retry";
  }

  private async awaitRewind(): Promise<"go" | "retry"> {
    const end = this.since + REQUEST_TIMEOUT_MS;
    while (Date.now() < end) {
      if (this.httpFailure) {
        await this.abort(`request rejected: ${this.httpFailure}`);
        return "retry";
      }
      const st = await this.status();
      if (st && st.stage === "new_turn") {
        this.phase = "new_turn";
        this.since = Date.now();
        this.decisionsInNewTurn = 0;
        this.report.versions.push(`rewound at v${await this.d.version()}`);
        this.d.log(`  redo: rewound, the new turn is played with the normal heuristics`);
        return "go";
      }
      const err = await this.d.pages[this.seatIndex]
        .getByTestId("turn-redo-error")
        .first()
        .innerText({ timeout: 300 })
        .catch(() => "");
      if (err) {
        await this.abort(`UI error: ${err.slice(0, 200)}`);
        return "retry";
      }
      await new Promise((r) => setTimeout(r, 400));
    }
    await this.abort("no redo status after the request");
    return "retry";
  }

  private async watchNewTurn(): Promise<"go" | "retry"> {
    const st = await this.status();
    if (st === undefined) return "go";
    if (st === null) {
      this.report.notes.push("status cleared while the new turn was played");
      this.phase = "done";
      this.report.outcome = "status vanished during the new turn";
      this.d.finding("Turn redo: the redo status disappeared while the new turn was played");
      return "go";
    }
    if (st.stage === "auto_played") {
      this.phase = "waiting_handoff";
      return "retry";
    }
    if (st.turn_complete) {
      // Hold the playthrough still: the tab fires the auto-play and replaces the timeline.
      this.phase = "waiting_handoff";
      this.since = Date.now();
      return "retry";
    }
    if (Date.now() - this.since > NEW_TURN_TIMEOUT_MS || this.decisionsInNewTurn > 80) {
      await this.abort("the new turn did not complete in time");
      return "retry";
    }
    return "go";
  }

  private async awaitHandoff(): Promise<"go" | "retry"> {
    const page = this.d.pages[this.seatIndex];
    const start = Date.now();
    let st: RedoStatusJson | null | undefined;
    while (Date.now() - start < HANDOFF_TIMEOUT_MS) {
      st = await this.status();
      if (st && st.stage === "auto_played" && st.outcome) break;
      if (st === null) {
        await this.abort("status cleared while waiting for the auto-play");
        return "retry";
      }
      if (!this.replayClicked && Date.now() - start > 20_000) {
        // The tab should have fired the auto-play itself; ask once with the visible button.
        this.replayClicked = true;
        const clicked = await this.click(page, "turn-redo-replay");
        this.d.finding(
          `Turn redo: no auto-play handoff after 20 s; the 'Replay the round now' button was ${clicked ? "clicked" : "not available"}`,
        );
      }
      await new Promise((r) => setTimeout(r, 500));
    }
    if (!st || st.stage !== "auto_played" || !st.outcome) {
      await this.abort(`no auto-play handoff within ${HANDOFF_TIMEOUT_MS / 1000} s`);
      return "retry";
    }
    this.report.stopKind = st.outcome.stop.kind;
    this.report.keptDecisions = st.outcome.kept;
    this.report.versions.push(`auto-played at v${await this.d.version()}, stop=${st.outcome.stop.kind}, kept ${st.outcome.kept}/${st.outcome.tail_total}`);
    const bar = await page.getByTestId("turn-redo-bar").first().getAttribute("data-state").catch(() => null);
    this.report.notes.push(`bar state after handoff: ${bar}`);
    await this.d.shot(page, "redo-bar", `redo-handoff-bar`);
    const choice: "restore" | "keep" = this.d.rng() < 0.5 ? "restore" : "keep";
    this.report.choice = choice;
    const before = await this.d.version();
    const ok = await this.click(page, choice === "restore" ? "turn-redo-restore" : "turn-redo-keep");
    if (!ok) {
      await this.abort(`the ${choice} button was not clickable`);
      return "retry";
    }
    if (!(await this.waitStatusNull(FINISH_TIMEOUT_MS))) {
      await this.abort(`status still set ${FINISH_TIMEOUT_MS / 1000} s after ${choice}`);
      return "retry";
    }
    const after = await this.d.version();
    this.report.versions.push(`${choice} done: v${before} -> v${after}`);
    if (choice === "restore" && after <= before)
      this.d.finding(`Turn redo: restore did not raise the game version (v${before} -> v${after})`);
    this.report.outcome = `completed (${choice})`;
    this.phase = "done";
    return "go";
  }
}
