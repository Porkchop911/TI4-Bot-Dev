import type { Page } from "@playwright/test";
import { AUTO_PLAY_DELAY_MS, AUTO_PLAY_HOLD_LIMIT_MS } from "../src/hooks/useSecondaryAutoPlay.ts";

/**
 * Secondary pre-planning exercise. When a seat waits while another seat resolves a strategy card
 * (its "Prepare your secondary" chip is on screen), the harness decides per opportunity whether to
 * play the secondary the normal way or to PLAN it: open prepare mode, answer the stand-in question
 * with the usual UI elements, save the plan, then either let the AUTO mode play it silently or
 * confirm it in REVIEW mode when the real question opens.
 *
 * Env: TI4_SMOKE_PREP=0 disables; TI4_SMOKE_PREP_PROBABILITY (default 0.5) is the chance to plan
 * per opportunity; TI4_SMOKE_PREP_AUTO_PROBABILITY (default 0.9) the chance a plan uses Auto.
 * Any problem is a finding and the decision is played normally, so a session is never stuck.
 */

export interface PrepConfig {
  enabled: boolean;
  probability: number;
  autoProbability: number;
}

const fraction = (value: string | undefined, fallback: number) => {
  if (value === undefined || value === "") return fallback;
  const n = Number.parseFloat(value);
  return Number.isFinite(n) ? Math.min(1, Math.max(0, n)) : fallback;
};

export function prepConfigFromEnv(env: NodeJS.ProcessEnv = process.env): PrepConfig {
  const off = env.TI4_SMOKE_PREP === "0" || env.TI4_SMOKE_PREP?.toLowerCase() === "false";
  return {
    enabled: !off,
    probability: fraction(env.TI4_SMOKE_PREP_PROBABILITY, 0.5),
    autoProbability: fraction(env.TI4_SMOKE_PREP_AUTO_PROBABILITY, 0.9),
  };
}

export interface PrepReport {
  /** Times a seat's chip was on screen at the start of another seat's decision (once per card). */
  opportunities: number;
  planned: number;
  plannedAuto: number;
  plannedReview: number;
  /** Plans that were opened but ended without a saved plan. */
  noPlan: number;
  /** Prepared answers the Auto mode played silently (decision dialog never opened). */
  autoPlayed: number;
  /** Review-mode bars confirmed with one click. */
  reviewConfirmed: number;
  /** Review-mode "Choose myself" (then played normally). */
  chooseMyself: number;
  needsReview: number;
  /** Plans whose own window the engine never opened for that seat (Diplomacy: nothing exhausted, Trade: commodities full, cannot pay, ...); the client drops them when the action ends. Not a failure. */
  windowNeverOpened: number;
  /** Planned decisions that had to be played normally after something was off. */
  fallbacks: number;
  /** Follow-up steps (technology, planets, site) answered from the plan, Auto or Review. */
  followUps: number;
  /** One line per planned case: seat, round, card, mode, plan, outcome. */
  cases: string[];
  /** The same counters per card (key: the card's name, e.g. "Leadership"), plus `withPayment` for plans that carry an exact payment. */
  byCard: Record<string, Record<string, number>>;
}

export const emptyPrepReport = (): PrepReport => ({
  opportunities: 0,
  planned: 0,
  plannedAuto: 0,
  plannedReview: 0,
  noPlan: 0,
  autoPlayed: 0,
  reviewConfirmed: 0,
  chooseMyself: 0,
  needsReview: 0,
  windowNeverOpened: 0,
  fallbacks: 0,
  followUps: 0,
  cases: [],
  byCard: {},
});

interface Cand {
  idx: number;
  desc: string;
  full: string;
}

export interface PrepDeps {
  pages: Page[];
  rng: () => number;
  config: PrepConfig;
  log: (line: string) => void;
  finding: (text: string) => void;
  shot: (page: Page, key: string, name: string) => Promise<void>;
  collect: (page: Page) => Promise<Cand[]>;
  pickOne: (cands: Cand[], clicks: number) => Cand;
  /** Label put into findings so they carry their evidence (game seed, click seed). */
  label: string;
  /** The server's view of the seat: who `view.active_player` is (evidence for findings). */
  activeInfo?: (seat: number) => Promise<string>;
  /** The public event log's details as the server has them now (to tell a cancelled action from a dropped plan). */
  eventDetails?: (seat: number) => Promise<string[]>;
}

/** Coup d'Etat cancels the strategic action before anything resolves; the card is then played again as a NEW action. */
const COUP = /\bplayed Coup d.Etat\b/i;

interface Plan {
  seat: number;
  round: number;
  card: string;
  mode: "auto" | "review";
  summary: string;
  /** The secondary question was seen and handled; later decisions are follow-up steps. */
  started: boolean;
  line: number;
  /** When the client dropped the saved plan before its window opened (evidence). */
  lost?: string;
  /** How many log events the server had when the plan was saved. */
  eventsAtPlan?: number;
  /** The plan's own window was answered (auto-played or confirmed); what follows is optional follow-up. */
  answered?: boolean;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export class SecondaryPrepExercise {
  readonly report = emptyPrepReport();
  private seen = new Set<string>();
  private plans = new Map<number, Plan>();

  constructor(private readonly d: PrepDeps) {}

  /** Counts `key` for the card (the card's name, as the chip shows it). */
  private bump(card: string, key: string) {
    const name = /leadership/i.test(card) ? "Leadership" : card;
    const counters = (this.report.byCard[name] ??= {});
    counters[key] = (counters[key] ?? 0) + 1;
  }

  private perKind = new Map<string, number>();
  /** Records a finding; only the first two of each kind carry a line of their own (the counters have the rest). */
  private find(p: Plan, text: string, kind = text.slice(0, 24)) {
    const n = (this.perKind.get(kind) ?? 0) + 1;
    this.perKind.set(kind, n);
    if (n > 2) return;
    this.d.finding(`Secondary prep (seat ${p.seat + 1}, round ${p.round}, ${p.card}, ${p.mode}; ${this.d.label}): ${text}`);
  }

  /** What the seat's tab shows around the prepare chrome, as evidence for a finding. */
  private async evidence(page: Page): Promise<string> {
    return page
      .evaluate(() => {
        const text = (id: string, n: number) =>
          (document.querySelector(`[data-testid="${id}"]`)?.textContent ?? "").trim().replace(/\s+/g, " ").slice(0, n);
        const mode = document.querySelector('[data-testid="secondary-prep-mode"]')?.getAttribute("data-mode");
        let stored = "";
        try {
          for (let i = 0; i < localStorage.length; i++) {
            const k = localStorage.key(i) ?? "";
            if (k.startsWith("ti4_secondary_prepared")) stored += `${k.slice(-12)}=${(localStorage.getItem(k) ?? "").slice(0, 120)} `;
          }
        } catch {
          stored = "n/a";
        }
        return `mode=${mode} stored=[${stored}] prep="${text("secondary-prep", 160)}" dialog="${text("pending-choice-dialog", 100)}"`;
      })
      .catch(() => "evidence unavailable");
  }

  private async visible(page: Page, id: string): Promise<boolean> {
    return page.getByTestId(id).first().isVisible().catch(() => false);
  }

  /** At the start of `actor`'s decision: offer every other seat that shows the chip. */
  async offer(actor: number, round: number, decisions: number, prompt = ""): Promise<void> {
    if (!this.d.config.enabled) return;
    // A saved plan whose chip is gone before its window opened was dropped by the client: note when.
    for (const [seat, p] of this.plans) {
      if (p.started || p.lost || seat === actor) continue;
      const chip = this.d.pages[seat].getByTestId("secondary-prep-chip").first();
      const text = ((await chip.textContent({ timeout: 300 }).catch(() => "")) ?? "").trim();
      if (!/^Prepared/.test(text)) {
        p.lost = `plan no longer shown at decision #${decisions} (seat ${actor + 1} deciding "${prompt.slice(0, 50)}"; chip: "${text.slice(0, 40)}")`;
        this.report.cases[p.line] += ` [${p.lost}]`;
      }
    }
    for (const [seat, page] of this.d.pages.entries()) {
      if (seat === actor) continue;
      let note: string | null = null;
      try {
        const chip = page.getByTestId("secondary-prep-chip").first();
        if (!(await chip.isVisible().catch(() => false))) continue;
        note = ((await chip.textContent({ timeout: 500 })) ?? "").trim().replace(/\s+/g, " ");
      } catch {
        continue;
      }
      if (/^Prepared/.test(note)) continue;
      const card = /(?:Prepare your secondary)?\s*(.*?),? played by/.exec(note)?.[1]?.trim() || note.slice(0, 40);
      const key = `${seat}|${round}|${card}`;
      if (this.seen.has(key)) continue;
      this.seen.add(key);
      this.report.opportunities++;
      if (this.d.rng() >= this.d.config.probability) continue;
      const mode = this.d.rng() < this.d.config.autoProbability ? "auto" : "review";
      await this.plan(seat, page, round, card, mode, decisions).catch(async (err) => {
        this.d.finding(
          `Secondary prep (seat ${seat + 1}, round ${round}, ${card}; ${this.d.label}): planning threw ${err instanceof Error ? err.message.split("\n").filter((l) => l.trim() && !/^\s*-\s*(waiting for|attempting|scrolling|done scrolling|element is visible|element is stable|element is enabled|retrying)/.test(l)).slice(0, 4).map((l) => l.trim().slice(0, 240)).join(" | ") : err}`,
        );
        await this.d.shot(page, "prep-threw", `prep-planning-threw-seat${seat + 1}-r${round}`);
        this.report.fallbacks++; this.bump(card, "fallbacks");
        await page.keyboard.press("Escape").catch(() => {});
        const save = page.getByTestId("prep-save").first();
        if (await save.isVisible().catch(() => false)) await save.click({ timeout: 2_000 }).catch(() => {});
      });
    }
  }

  private async setMode(page: Page, mode: "auto" | "review"): Promise<boolean> {
    const button = page.getByTestId(`secondary-prep-mode-${mode}`).first();
    try {
      await button.scrollIntoViewIfNeeded({ timeout: 2_000 });
      await button.click({ timeout: 3_000 });
      const now = await page.getByTestId("secondary-prep-mode").first().getAttribute("data-mode");
      return now === mode;
    } catch {
      return false;
    }
  }

  private async plan(seat: number, page: Page, round: number, card: string, mode: "auto" | "review", decisions: number) {
    const p: Plan = { seat, round, card, mode, summary: "", started: false, line: this.report.cases.length };
    this.report.planned++;
    this.bump(card, "planned");
    this.bump(card, mode === "auto" ? "plannedAuto" : "plannedReview");
    if (mode === "auto") this.report.plannedAuto++;
    else this.report.plannedReview++;
    this.d.log(`  prep: seat ${seat + 1} plans its ${card} secondary (${mode}) at decision #${decisions}`);
    if (!(await this.setMode(page, mode))) {
      this.find(p, "could not set the Review/Auto switch on the player sheet");
      this.report.fallbacks++; this.bump(p.card, "fallbacks");
      return;
    }
    // The turn-redo exercise leaves the event log expanded, and it covers the board and the chip.
    const logToggle = page.getByTestId("event-log-toggle").first();
    if ((await logToggle.getAttribute("aria-expanded", { timeout: 500 }).catch(() => null)) === "true") {
      await logToggle.click({ timeout: 2_000 }).catch(() => {});
    }
    await page.getByTestId("secondary-prep-chip").first().click({ timeout: 3_000 });
    const banner = page.getByTestId("prepare-banner").first();
    if (!(await banner.waitFor({ state: "visible", timeout: 4_000 }).then(() => true).catch(() => false))) {
      this.find(p, "the prepare banner never appeared after clicking the chip");
      this.report.fallbacks++; this.bump(p.card, "fallbacks");
      return;
    }
    await this.d.shot(page, "prep-banner", `prep-${card.toLowerCase()}-seat${seat + 1}-prepare-mode`.slice(0, 100));
    const clicked: string[] = [];
    for (let i = 0; i < 12; i++) {
      if (!(await this.visible(page, "prepare-banner"))) break;
      let cands = await this.d.collect(page);
      for (let w = 0; w < 15 && !cands.length && (await this.visible(page, "prepare-banner")); w++) {
        await sleep(150);
        cands = await this.d.collect(page);
      }
      if (!cands.length) {
        // The banner is still open but nothing in it can be clicked: say what the tab shows.
        if (await this.visible(page, "prepare-banner")) {
          const shown = await page
            .evaluate(() => {
              const ids = [...document.querySelectorAll("[data-testid]")]
                .map((el) => el.getAttribute("data-testid") ?? "")
                .filter((id) => /drawer|dialog|modal|panel|bar|tray/.test(id))
                .slice(0, 12);
              return `${window.innerWidth}x${window.innerHeight} containers=[${ids.join(",")}]`;
            })
            .catch(() => "n/a");
          this.find(p, `prepare mode is open but offers nothing to click after ${clicked.join(", ") || "no clicks"} (${shown}; ${await this.evidence(page)})`, "prepare mode is open but");
        }
        break;
      }
      const yes = cands.find((c) => /secondary-yes-btn/.test(c.desc));
      const skip = cands.find((c) => /secondary-skip-btn/.test(c.desc));
      const chosen = yes && skip ? (this.d.rng() < 0.75 ? yes : skip) : this.d.pickOne(cands, i);
      clicked.push(chosen.desc.split(" | ")[0] || chosen.desc.slice(0, 30));
      await page.locator(`[data-smoke-idx="${chosen.idx}"]`).click({ timeout: 2_000 }).catch(() => {});
      await sleep(250);
    }
    // Save plan (a plan that finished its last step has already closed the mode by itself).
    const save = page.getByTestId("prep-save").first();
    if (await save.isVisible().catch(() => false)) {
      await save.click({ timeout: 3_000 }).catch(() => {});
      await banner.waitFor({ state: "hidden", timeout: 3_000 }).catch(() => {});
    }
    if (await this.visible(page, "prepare-banner")) {
      this.find(p, "prepare mode did not close after Save plan");
      this.report.fallbacks++; this.bump(p.card, "fallbacks");
      return;
    }
    const chipText = ((await page.getByTestId("secondary-prep-chip").first().textContent({ timeout: 1_000 }).catch(() => "")) ?? "").trim();
    if (!/^Prepared/.test(chipText)) {
      this.report.noPlan++; this.bump(p.card, "noPlan");
      this.report.cases.push(`seat ${seat + 1} r${round} ${card} ${mode}: no plan saved (clicks: ${clicked.join(", ")})`);
      this.find(p, `no plan was saved after answering the stand-in question (clicks: ${clicked.join(", ")})`);
      return;
    }
    p.summary = chipText.replace(/^Prepared\s*/, "").slice(0, 160);
    if (/ paying /.test(p.summary)) this.bump(card, "withPayment");
    p.line = this.report.cases.push(`seat ${seat + 1} r${round} ${card} ${mode}: plan "${p.summary}" (clicks: ${clicked.join(", ")}) -> pending`) - 1;
    p.eventsAtPlan = (await this.d.eventDetails?.(seat).catch(() => undefined))?.length;
    this.plans.set(seat, p);
  }

  /**
   * A later decision of the seat found nothing to do for the plan. When the plan's own window was
   * answered, its outcome stays (auto-played / confirmed): a plan that records no follow-up step
   * (no technology, planet or site: "Follow Technology" and nothing else) simply has none left.
   */
  private planEnded(p: Plan) {
    if (!p.answered) this.setOutcome(p, "plan ended (follow-up played normally)");
  }

  private setOutcome(p: Plan, outcome: string) {
    this.report.cases[p.line] = this.report.cases[p.line].replace(/-> .*$/, `-> ${outcome}`);
  }

  /**
   * At the start of a decision for `seat`: when a plan covers it, let Auto answer it or confirm the
   * review bar. Returns "done" when the decision was answered (version moved) and "normal" when the
   * caller has to play it with the usual heuristics.
   */
  async handle(
    seat: number,
    choice: { details?: Record<string, unknown>; prompt?: string },
    round: number,
    before: number,
    version: () => Promise<number>,
  ): Promise<"done" | "normal"> {
    const p = this.plans.get(seat);
    if (!p) return "normal";
    const page = this.d.pages[seat];
    const details = choice.details as { kind?: string; card?: string } | undefined;
    const secondary = details?.kind === "strategy_secondary";
    if (p.round !== round) {
      this.plans.delete(seat);
      return "normal";
    }
    if (!p.started) {
      if (!secondary) return "normal"; // some other decision first (a reaction): not this plan's window
      // The plan's own secondary may never open for this seat (the engine withholds a secondary that
      // could do nothing, e.g. Diplomacy with nothing exhausted); a LATER card's secondary in the same
      // round is then a different action's window, not this plan's.
      if (details?.card && !details.card.toLowerCase().includes(p.card.toLowerCase())) {
        this.report.windowNeverOpened++;
        this.setOutcome(p, `window never opened (the next secondary is ${details.card})`);
        this.plans.delete(seat);
        return "normal";
      }
      // Coup d'Etat cancelled the action the plan was made for (the client rightly drops the plan);
      // the primary then plays the same card again, and THAT action's window is not this plan's.
      if (p.lost && p.eventsAtPlan !== undefined) {
        const details = await this.d.eventDetails?.(seat).catch(() => undefined);
        if (details?.slice(p.eventsAtPlan).some((line) => COUP.test(line))) {
          this.report.windowNeverOpened++;
          this.setOutcome(p, "action cancelled by Coup d'Etat (this window belongs to the card's replay)");
          this.plans.delete(seat);
          return "normal";
        }
      }
      p.started = true;
    }
    // A secondary of another card after this plan's own window was answered (and no follow-up step
    // came) is a different action's window: the plan is over (the case line keeps its outcome).
    if (p.started && secondary && details?.card && !details.card.toLowerCase().includes(p.card.toLowerCase())) {
      this.plans.delete(seat);
      return "normal";
    }
    const first = secondary;
    if (!first && (choice.prompt === "action phase" || /^end your turn/i.test(choice.prompt ?? ""))) {
      this.plans.delete(seat); // the plan's follow-up steps are over; this is the seat's own turn
      return "normal";
    }
    // A follow-up step of an Auto plan is sent after the toast delay and its hold can last up to the hold limit.
    const limit = first ? 20_000 : AUTO_PLAY_DELAY_MS + AUTO_PLAY_HOLD_LIMIT_MS + 1_500;
    const start = Date.now();
    let sawAutoToast = false;
    let dialogBeforeAnswer = false;
    let reviewBar = false;
    let needs = false;
    let confirmed = false;
    let clickWhy = "";
    while (Date.now() - start < limit) {
      if ((await version()) > before) break;
      if (p.mode === "auto") {
        if ((await this.visible(page, "secondary-autoplay-toast")) || (await this.visible(page, "secondary-autoplayed-toast"))) sawAutoToast = true;
        if (!sawAutoToast && ((await this.visible(page, "pending-choice-dialog")) || (await this.visible(page, "strategy-secondary-panel"))))
          dialogBeforeAnswer = true;
        if (dialogBeforeAnswer && Date.now() - start > 4_000 && !sawAutoToast) break; // it opened for real: play it
      } else {
        if (await this.visible(page, "secondary-prepared-bar")) {
          reviewBar = true;
          break;
        }
        if (await this.visible(page, "secondary-needs-review")) {
          needs = true;
          break;
        }
      }
      await sleep(150);
    }
    const moved = (await version()) > before;
    if (p.mode === "auto") {
      if (moved) {
        if (first) { this.report.autoPlayed++; this.bump(p.card, "autoPlayed"); p.answered = true; }
        else this.report.followUps++;
        if (!sawAutoToast) {
          // The played toast shows for 5 s after the answer; give the last frame a moment.
          sawAutoToast = (await this.visible(page, "secondary-autoplayed-toast")) || (await this.visible(page, "secondary-autoplay-toast"));
        }
        if (!sawAutoToast) this.find(p, "the prepared answer was played but no 'Auto-played your prepared secondary' toast was seen");
        if (dialogBeforeAnswer) this.find(p, "a decision dialog opened although Auto was set and a plan was saved");
        this.setOutcome(p, `${first ? "auto-played" : "auto follow-up"}${sawAutoToast ? " (toast seen)" : " (NO toast)"}`);
        if (!first) this.plans.delete(seat);
        return "done";
      }
      if (first) {
        this.find(p, `Auto did not answer the secondary within ${limit / 1000} s (toast seen: ${sawAutoToast}, dialog opened: ${dialogBeforeAnswer}; ${await this.evidence(page)}; ${p.lost ?? "plan was never seen dropped"}; ${this.d.activeInfo ? await this.d.activeInfo(seat) : ""})`);
        await this.d.shot(page, "prep-auto-miss", `prep-auto-did-not-fire-seat${seat + 1}-r${round}`);
      }
      if (first) this.setOutcome(p, "AUTO DID NOT FIRE (played normally)");
      else this.planEnded(p);
      if (first) { this.report.fallbacks++; this.bump(p.card, "fallbacks"); }
      this.plans.delete(seat);
      return "normal";
    }
    // Review mode
    if (needs) {
      this.report.needsReview++; this.bump(p.card, "needsReview");
      const reason = ((await page.getByTestId("secondary-needs-review").first().textContent({ timeout: 500 }).catch(() => "")) ?? "").trim().slice(0, 160);
      this.find(p, `the saved plan showed 'Needs review' when the real question opened: ${reason}`);
      this.setOutcome(p, "needs review (played normally)");
      this.plans.delete(seat);
      return "normal";
    }
    if (!reviewBar) {
      if (moved) return "done";
      if (first) {
        this.find(p, `no 'Prepared: ...' confirm bar within ${limit / 1000} s of the real question opening`);
        this.report.fallbacks++; this.bump(p.card, "fallbacks");
      }
      if (first) this.setOutcome(p, "NO REVIEW BAR (played normally)");
      else this.planEnded(p);
      this.plans.delete(seat);
      return "normal";
    }
    if (this.d.rng() < 0.25) {
      await page.getByTestId("secondary-prepared-dismiss").first().click({ timeout: 3_000 }).catch(() => {});
      this.report.chooseMyself++; this.bump(p.card, "chooseMyself");
      this.setOutcome(p, "choose myself (played normally)");
      this.plans.delete(seat);
      return "normal";
    }
    try {
      await page.getByTestId("secondary-prepared-confirm").first().click({ timeout: 4_000 });
      confirmed = true;
    } catch {
      confirmed = false;
      // Say why: a disabled button (a request still running) or something lying over it.
      const why = await page
        .evaluate(() => {
          const el = document.querySelector('[data-testid="secondary-prepared-confirm"]') as HTMLButtonElement | null;
          if (!el) return "the button is gone";
          const r = el.getBoundingClientRect();
          const top = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
          const over = top && top !== el && !el.contains(top) ? `${top.tagName}[data-testid=${top.getAttribute("data-testid")}].${String(top.className).slice(0, 60)}` : "nothing";
          return `disabled=${el.disabled} box=${Math.round(r.x)},${Math.round(r.y)},${Math.round(r.width)}x${Math.round(r.height)} covered-by=${over}`;
        })
        .catch((e: unknown) => `evaluate failed: ${String(e).slice(0, 80)}`);
      clickWhy = why;
      this.d.log(`  prep: confirm click failed (${why})`);
    }
    const end = Date.now() + 12_000;
    while (confirmed && Date.now() < end && (await version()) <= before) await sleep(150);
    if (confirmed && (await version()) > before) {
      if (first) { this.report.reviewConfirmed++; this.bump(p.card, "reviewConfirmed"); p.answered = true; }
      else this.report.followUps++;
      this.setOutcome(p, first ? "review confirmed" : "review follow-up confirmed");
      if (!first) this.plans.delete(seat);
      return "done";
    }
    this.find(p, confirmed ? "Confirm was clicked but the decision did not advance in 12 s" : `the Confirm button could not be clicked (${clickWhy})`);
    this.report.fallbacks++; this.bump(p.card, "fallbacks");
    this.setOutcome(p, "CONFIRM FAILED (played normally)");
    this.plans.delete(seat);
    return "normal";
  }
}
