import { useEffect, useMemo, useRef, useState } from "react";
import type { GameSession, Intent } from "../model";
import {
  type Draft,
  type DraftEntry,
  STAGES,
  type Stage,
  activate,
  answer,
  canRedo,
  canUndo,
  entryOf,
  frontier,
  isBoundary,
  newDraft,
  parseDraft as parsePlan,
  problemOf,
  redo,
  scriptOf,
  setLandings,
  setMovement,
  setProduction,
  stageAfter,
  startOver,
  stepsOfStage,
  trimDraft,
  undo,
} from "./draft";
import {
  type LandingDraft,
  chosenForce,
  forceOfId,
  draftOf as landingOf,
  stepsOf as landingSteps,
  setCount as setLandingCount,
  setLanding,
} from "./landingDraft";
import { type MovementDraft, fill, poolOfSource, resetOrigin, setCount } from "./movementDraft";
import { type KeptOdds, type OddsRequest, oddsRequest } from "./movementOdds";
import { type MovementPlan, type PlanStep, draftOf, stageOf, stepsOf } from "./movementPlan";
import {
  GOODS,
  EMPTY as NO_PRODUCTION,
  type ProductionDraft,
  autoPay,
  clearPayment,
  isEmpty,
  overrunOf,
  draftOf as productionOf,
  stepsOf as productionSteps,
  setGoods,
  setCount as setProductionCount,
  togglePlanet,
  totals,
} from "./productionDraft";
import {
  MAX_STAGED_BYTES,
  type SeatStore,
  type StagedSlot,
  sameStaged,
  stagedBytes,
} from "./seatStore";
import {
  type SecondaryDraft,
  asksSecondary,
  endOf,
  isOf,
  isOver,
  newDraft as newFollow,
  problemOf as followProblem,
  parseDraft as parseFollow,
  record,
  scopeOf,
  scriptOf as followScriptOf,
} from "./secondaryDraft";
import {
  type DraftInfo,
  type FollowInfo,
  type LocalState,
  optionOfSeat,
  optionOnBoard,
} from "./select/action";
import type { CardSettings } from "./select/cards";
import { systemLabel } from "./select/movement";
import { selectShell } from "./select/shell";
import { tacticalFacts } from "./select/tactical";
import {
  countPurchase,
  payPurchase,
  purchasePlan,
  selectPurchase,
  setPurchaseGoods,
  togglePurchasePlanet,
} from "./select/purchase";
import {
  chooseReadySystem,
  clearLastReady,
  clearReadyPart,
  selectReady,
  toggleReadyPlanet,
} from "./select/ready";
import { countPools, selectTokens } from "./select/tokens";
import type { Transport } from "./transport";
import type { DraftOutcome, DraftStep, PlanScope, ReactionModes, SessionUpdate } from "./wire";

const NOTHING: LocalState = {
  inspected: null,
  staged: null,
  pools: null,
  purchase: null,
  ready: null,
  sent: null,
  error: null,
  replaying: null,
  canUndo: false,
  movement: {},
  landing: {},
  landingKind: null,
  production: NO_PRODUCTION,
  productionStage: "build",
  handled: [],
  step: null,
  remaining: null,
  planNote: null,
  stepping: null,
  draftLocked: null,
  draft: null,
  odds: null,
  menu: null,
};

/** The label of a system as the activation step lists it, for the search field. */
const sameSystem = (query: string, id: string) =>
  query.trim() === id || query.trim().endsWith(`#${id}`);

/** A script that the game is asked to play, and what it is of. No scope: a tactical action. */
interface Wanted {
  script: DraftStep[];
  scope?: PlanScope;
}

/** What the game made of a draft script, and the update of the game that it was asked on. */
interface DraftRun {
  wanted: Wanted;
  base: SessionUpdate;
  /** Null: the game did not wait for this side. */
  outcome: DraftOutcome | null;
}

const same = (a: unknown, b: unknown) => a === b || JSON.stringify(a) === JSON.stringify(b);

/** The note of a ready draft of a secondary that the game no longer plays whole. */
const UNFIT = { text: "Your draft no longer fits the game. It is not ready.", error: true };

const readFollowOf = (store: SeatStore | undefined): SecondaryDraft | null =>
  store ? parseFollow(store.get("secondary")) : null;

/** The private draft of the page, and whether it is in view. */
interface Plan {
  draft: Draft | null;
  open: boolean;
  /** The step of the draft that the player chose to see. Null: where the draft is open. */
  stage: Stage | null;
}

const readPlanOf = (store: SeatStore | undefined): Plan => ({
  draft: store ? parsePlan(store.get("plan")) : null,
  open: false,
  stage: null,
});

/** The draft as it is kept: within the bound of a slot of the server. */
const keptPlan = (draft: Draft | null) => draft && trimDraft(draft, MAX_STAGED_BYTES, stagedBytes);

/** The steps of a plan that belong to the steps of the action after `stage`. Null when there are none. */
function laterSteps(plan: MovementPlan | null, stage: Stage): MovementPlan | null {
  const steps = (plan?.steps ?? []).filter((step) => {
    const of = stageOf(step);
    return of !== null && STAGES.indexOf(of) > STAGES.indexOf(stage);
  });
  return plan && steps.length ? { ...plan, steps } : null;
}

/** Why the viewer cannot draft a tactical action now. Null when the Draft tab opens. */
function draftLock(update: SessionUpdate, local: LocalState): string | null {
  const seat = update.viewer.role === "player" ? update.viewer.seat : null;
  if (local.error || local.replaying || update.view.finished) {
    return "The game does not wait for you";
  }
  if (seat === null) {
    return "A draft is private to a player";
  }
  // In the strategy phase the draft is of the turn that comes: the board is the same.
  if (update.view.phase !== "action" && update.view.phase !== "strategy") {
    return "A draft is of the action phase";
  }
  if (update.view.players.find((player) => player.id === seat)?.passed) {
    return "You have passed";
  }
  return null;
}

/** The turn menu of the viewer, with the tactical action in it: where a draft is sent. */
const menuNonce = (update: SessionUpdate, local: LocalState): string | null => {
  const pending = update.pending_choice;
  return pending &&
    pending.nonce !== local.sent &&
    !local.offline &&
    pending.choice.context?.subtype === "action_menu" &&
    pending.choice.options.some((option) => option.id === "tactical")
    ? pending.nonce
    : null;
};

/** What "Apply to Live" sends, in words: the steps of the draft that the game played. */
function applyItems(update: SessionUpdate, system: string, sent: PlanStep[], entry: DraftEntry) {
  const tiles = new Map((update.view.board.map_tiles ?? []).map((tile) => [tile.system_id, tile]));
  const items = [`Activate ${systemLabel(tiles, system)}`];
  const has = (stage: Stage) => sent.some((step) => stageOf(step) === stage);
  if (!has("movement")) {
    return [...items, "The movement is decided in Live"];
  }
  const origins = new Map<string, { ships: number; units: number }>();
  let last: { ships: number; units: number } | null = null;
  for (const step of sent) {
    if (step.kind === "move") {
      last = origins.get(step.origin) ?? { ships: 0, units: 0 };
      last.ships += 1;
      origins.set(step.origin, last);
    } else if (step.kind === "load" && last) {
      last.units += 1;
    }
  }
  for (const [origin, { ships, units }] of origins) {
    const cargo = units ? ` with ${units} ${units === 1 ? "unit" : "units"}` : "";
    items.push(
      `${ships} ${ships === 1 ? "ship" : "ships"} from ${systemLabel(tiles, origin)}${cargo}`,
    );
  }
  if (!origins.size) {
    items.push("No ship moves");
  }
  const count = (kind: PlanStep["kind"]) => sent.filter((step) => step.kind === kind).length;
  if (has("invasion")) {
    const lands = count("land");
    items.push(
      lands ? `${lands} ground ${lands === 1 ? "force lands" : "forces land"}` : "Nothing lands",
    );
  }
  if (has("production")) {
    const builds = count("produce");
    items.push(
      builds
        ? `${builds} ${builds === 1 ? "purchase" : "purchases"} produced and paid`
        : "Nothing is produced",
    );
  }
  // What the draft holds after the point where the game stops to roll or to ask.
  const later = STAGES.filter((stage) => !has(stage) && stepsOfStage(entry, stage) !== null);
  if (later.length) {
    const names = { movement: "movement", invasion: "landings", production: "production" };
    items.push(`Staged again in Live: ${later.map((stage) => names[stage]).join(", ")}`);
  }
  return items;
}

/** The draft in view: the update that its screens are made from, and what its toolbar says. */
function draftView(
  live: SessionUpdate,
  local: LocalState,
  draft: Draft,
  outcome: DraftOutcome,
  chosen: Stage | null,
  /** The outcome is of an older script, or of the game before its last update. */
  waiting: boolean,
): { update: SessionUpdate; local: LocalState; stage: Stage | null; sent: PlanStep[] } {
  const entry = entryOf(draft);
  const problem = problemOf(entry, outcome);
  // The step in view: the one that the player chose, while the game still asks for it.
  const stage =
    entry.system === null ? null : chosen && outcome[chosen] ? chosen : frontier(outcome);
  const update = (stage && outcome[stage]) ?? outcome.update;
  const facts = update.tactical ?? null;
  // A roll ends the check, not the draft: the game goes on from there in Live. So does a
  // question that the draft has no answer for yet.
  const sound =
    outcome.stop.kind === "open" || outcome.stop.kind === "complete" || isBoundary(outcome);
  const atMenu = menuNonce(live, local) !== null;
  const canApply = entry.system !== null && sound && atMenu && !waiting;
  // The action and its system are sent as choices; the rest is the plan.
  const sent = scriptOf(entry).slice(2, Math.max(2, outcome.consumed)) as PlanStep[];
  const recorded = stage !== null && stepsOfStage(entry, stage) !== null;
  const next =
    stage && recorded && !(stage === "production" && entry.unpaid)
      ? stageAfter(outcome, stage)
      : null;
  const info: DraftInfo = {
    canUndo: canUndo(draft),
    canRedo: canRedo(draft),
    canApply,
    applyHint:
      entry.system === null
        ? "Choose a system first"
        : waiting
          ? "The game checks the draft"
          : !sound
            ? (problem ?? "The draft does not fit the game")
            : !atMenu
              ? "Apply when you choose your action"
              : "Apply the recorded choices to the live game",
    apply:
      canApply && entry.system !== null
        ? {
            items: applyItems(update, entry.system, sent, entry),
            cost: "Costs 1 tactic token.",
            note: "Dice and later decisions continue in Live.",
          }
        : null,
    problem,
    waiting,
    stage,
    reached: STAGES.filter((item) => outcome[item] !== undefined),
    recorded,
    next,
    ends:
      recorded && !next && outcome.stop.kind === "complete"
        ? "The action ends here. Every step is drafted."
        : null,
    fromMenu: atMenu,
  };
  const steps = (of: Stage) => stepsOfStage(entry, of) ?? [];
  return {
    update,
    stage,
    sent,
    local: {
      ...local,
      sent: null,
      canUndo: false,
      movement: facts?.kind === "movement" ? draftOf(facts, steps("movement")) : {},
      landing: facts?.kind === "invasion" ? landingOf(facts, steps("invasion")) : {},
      production:
        facts?.kind === "production" ? productionOf(facts, steps("production")) : NO_PRODUCTION,
      remaining: null,
      planNote:
        problem && (facts || entry.system !== null)
          ? {
              text: problem,
              error: outcome.stop.kind === "refused" || outcome.stop.kind === "failed",
            }
          : null,
      stepping: null,
      draft: info,
    },
  };
}

/**
 * The staged movement of one movement step, as the steps that "Move fleet" would send, or the
 * staged landings of one invasion step.
 */
interface SavedDraft {
  nonce: string;
  destination: string;
  steps: PlanStep[];
}

function parseSaved(value: unknown): SavedDraft | null {
  const saved = value as SavedDraft | null;
  return saved && typeof saved.nonce === "string" && Array.isArray(saved.steps) ? saved : null;
}

const readDraft = (store: SeatStore | undefined): SavedDraft | null =>
  store ? parseSaved(store.get("movement")) : null;

/** The steps of a staged movement, landings or production, as the game would be sent them. Null: nothing is staged. */
function stagedSteps(update: SessionUpdate, local: LocalState): PlanStep[] | null {
  const facts = update.tactical;
  return facts?.kind === "movement" && Object.keys(local.movement).length
    ? stepsOf(facts, local.movement)
    : facts?.kind === "invasion" && Object.keys(local.landing).length
      ? landingSteps(facts, local.landing)
      : facts?.kind === "production" && !isEmpty(local.production)
        ? productionSteps(facts, local.production)
        : null;
}

export interface LiveSession {
  /** Null until the first update. */
  session: GameSession | null;
  /** Why the game stopped, or cannot start. */
  error: string | null;
  /** A saved game is played again: how many of its answers are done. */
  replaying: { done: number; total: number } | null;
}

/**
 * A `GameSession` on a transport: the game comes from the updates, and what the player stages
 * between two updates is kept here. Only the main button sends something.
 * With `drafts`, what is staged is still staged after a reload, and a change of it in another
 * tab of the seat is staged here too. What is open on the screen is of this tab.
 */
export function useLiveSession(transport: Transport | null, drafts?: SeatStore): LiveSession {
  const [update, setUpdate] = useState<SessionUpdate | null>(null);
  const [local, setLocal] = useState<LocalState>(NOTHING);
  const [plan, setPlan] = useState<Plan>(() => readPlanOf(drafts));
  /** The private draft of the viewer's secondary. Null: nothing was answered yet. */
  const [follow, setFollow] = useState<SecondaryDraft | null>(() => readFollowOf(drafts));
  /** The modes that the viewer set for its reactions: a view of the transport. */
  const [modes, setModes] = useState<ReactionModes>({});
  /** The cards that the viewer acts as if holding. Of the page only. */
  const [faked, setFaked] = useState<string[]>([]);
  const cards = useMemo((): CardSettings => ({ modes, faked }), [modes, faked]);
  const [toast, setToast] = useState<{ id: number; text: string } | null>(null);
  /** What the game made of the draft, as the transport told it last. */
  const [run, setRun] = useState<DraftRun | null>(null);
  /** The script that the transport was asked last, and the update of then. An answer for another script is late. */
  const asked = useRef<{ wanted: Wanted; base: SessionUpdate } | null>(null);
  /** The last update, for a change of another tab. */
  const latest = useRef<SessionUpdate | null>(null);
  /**
   * What this tab had staged in each slot when it last looked: a slot is written only when this
   * changes, so an effect of a render before a change of another tab does not write it back.
   * Undefined: not looked yet.
   */
  const shown = useRef<Partial<Record<StagedSlot, unknown>>>({});

  useEffect(() => {
    setUpdate(null);
    setLocal(NOTHING);
    setPlan(readPlanOf(drafts));
    setFollow(readFollowOf(drafts));
    setModes({});
    setFaked([]);
    setRun(null);
    asked.current = null;
    latest.current = null;
    shown.current = {};
    /** The step and the system whose sheet (the landings, the build) opened with the last update. */
    let sheetAt: { key: string; system: string } | null = null;
    /** What the inspector showed before the landings opened. */
    let inspectedBefore: string | null = null;
    /** The question of the viewer at the last update. Undefined: the game asked the viewer nothing. */
    let choiceAt: string | undefined;
    return transport?.subscribe((event) => {
      if (event.kind === "error") {
        setLocal((now) => ({ ...now, staged: null, replaying: null, error: event.message }));
        return;
      }
      if (event.kind === "replaying") {
        const { done, total } = event;
        setLocal((now) => ({
          ...now,
          staged: null,
          sent: null,
          error: null,
          replaying: { done, total },
        }));
        return;
      }
      if (event.kind === "draft") {
        // An answer for an older script is dropped: the newer one is on its way.
        const last = asked.current;
        if (
          last &&
          same(event.script, last.wanted.script) &&
          same(event.scope ?? null, last.wanted.scope ?? null)
        ) {
          setRun({ ...last, outcome: event.outcome });
        }
        return;
      }
      if (event.kind === "modes") {
        return setModes(event.modes);
      }
      if (event.kind === "passed") {
        const { cards: passed } = event;
        return setToast((now) => ({
          id: (now?.id ?? 0) + 1,
          text: `${passed.join(", ")}: not offered now. The window was passed.`,
        }));
      }
      if (event.kind === "status") {
        const { status } = event;
        return setUpdate((now) => now && { ...now, turn_status: status });
      }
      // An answer that was lost or refused is not sent: the next update has the same question,
      // and it can be answered again.
      if (event.kind === "offline") {
        return setLocal((now) => ({ ...now, offline: true, sent: null }));
      }
      if (event.kind === "rejected") {
        const { reason } = event;
        setLocal((now) => ({ ...now, sent: null }));
        return setToast((now) => ({ id: (now?.id ?? 0) + 1, text: reason }));
      }
      setUpdate(event.update);
      latest.current = event.update;
      // The landings open at the active system when their step opens, and so does the build of
      // a production.
      const tactical = event.update.tactical;
      const sheet =
        tactical?.kind === "invasion"
          ? { key: `invasion:${tactical.active}`, system: tactical.active }
          : tactical?.kind === "production"
            ? { key: `production:${tactical.system}`, system: tactical.system }
            : null;
      const opens = sheet !== null && sheetAt?.key !== sheet.key;
      // When their step is over (sent, or taken back), the board is where the player left it.
      const closes = sheet === null ? (sheetAt?.system ?? null) : null;
      sheetAt = sheet;
      // An update of another seat leaves the question of the viewer as it was: what the player
      // staged, live or in a draft, stays.
      const fresh =
        event.update.pending_choice?.nonce !== choiceAt || !!event.plan || !!event.undone;
      choiceAt = event.update.pending_choice?.nonce;
      // A new choice starts with nothing staged. The inspector stays where the player left it.
      setLocal((now) => {
        if (opens) {
          inspectedBefore = now.inspected;
        }
        const inspected = opens
          ? (sheet?.system ?? null)
          : closes !== null && now.inspected === closes
            ? inspectedBefore
            : now.inspected;
        if (!fresh) {
          return {
            ...now,
            inspected,
            error: null,
            offline: false,
            replaying: null,
            canUndo: event.canUndo,
            stepping: event.stepping?.seat ?? null,
          };
        }
        const facts = event.update.tactical;
        const stopped = event.plan;
        // A movement that another decision stopped is staged again when the movement goes on.
        let remaining =
          stopped?.interrupted && stopped.remaining.length
            ? { destination: stopped.destination, steps: stopped.remaining }
            : now.remaining;
        // A movement that an undo took back is staged again, in front of what was still to move.
        const undone = event.undone;
        const active =
          facts?.kind === "movement" ? facts.active : event.update.view.board.active_system;
        if (undone && active) {
          const rest = remaining?.destination === active ? remaining.steps : [];
          remaining = { destination: active, steps: [...undone, ...rest] };
        }
        let movement = {};
        let landing: LandingDraft = {};
        let production: ProductionDraft = NO_PRODUCTION;
        let planNote =
          stopped && !stopped.interrupted ? { text: stopped.reason, error: true } : null;
        if (facts && facts.kind !== "activation") {
          const place = facts.kind === "production" ? facts.system : facts.active;
          const again = stopped && !stopped.interrupted ? stopped.remaining : remaining?.steps;
          // The steps that were staged for this choice before a reload.
          const saved = again ? null : readDraft(drafts);
          const steps =
            again && (stopped?.destination ?? remaining?.destination) === place
              ? again
              : saved?.nonce === event.update.pending_choice?.nonce && saved?.destination === place
                ? saved.steps
                : null;
          if (steps && facts.kind === "movement") {
            movement = draftOf(facts, steps);
          } else if (steps && facts.kind === "invasion") {
            landing = landingOf(facts, steps);
          } else if (steps && facts.kind === "production") {
            production = productionOf(facts, steps);
          }
          const stage = facts.kind;
          if (
            steps &&
            steps === again &&
            !undone &&
            steps.some((step) => stageOf(step) === stage)
          ) {
            planNote ??= {
              text: remaining?.drafted
                ? "Staged from your draft. Nothing of it is sent yet."
                : facts.kind === "movement"
                  ? "Another decision stopped the movement. What did not move is staged again."
                  : facts.kind === "invasion"
                    ? "Another decision stopped the landings. What did not land is staged again."
                    : "Another decision stopped the production. What was not produced is staged again.",
              error: false,
            };
          }
          // The steps of the action that come later wait for their own step.
          remaining = remaining?.destination === place ? laterSteps(remaining, stage) : null;
        } else if (facts?.kind === "activation" || !event.update.view.board.active_system) {
          // The action is over, or another one starts.
          remaining = null;
        }
        return {
          ...now,
          inspected,
          staged: null,
          pools: null,
          purchase: null,
          ready: null,
          sent: null,
          error: null,
          offline: false,
          replaying: null,
          canUndo: event.canUndo,
          movement,
          landing,
          landingKind: null,
          production,
          productionStage: "build",
          handled: [],
          step: null,
          remaining,
          planNote,
          stepping: event.stepping?.seat ?? null,
          odds: null,
          menu: null,
        };
      });
    });
  }, [transport, drafts]);

  // A change that another tab of the seat staged is staged here too. What is open on the
  // screen stays as this tab has it. What is applied is the value of the store, so it is not
  // written back.
  useEffect(
    () =>
      drafts?.subscribe((slot, value, server) => {
        if (slot === "plan") {
          const draft = parsePlan(value);
          return setPlan((now) =>
            sameStaged(keptPlan(now.draft), draft)
              ? now
              : draft
                ? { ...now, draft, stage: entryOf(draft).system === null ? null : now.stage }
                : { open: false, draft: null, stage: null },
          );
        }
        if (slot === "secondary") {
          const follow = parseFollow(value);
          if (server && follow && !follow.ready) {
            // The server sends a ready draft, and makes one that no longer fits not ready.
            setLocal((now) => ({ ...now, planNote: UNFIT }));
          }
          return setFollow((now) => (sameStaged(now, follow) ? now : follow));
        }
        // The staged steps of the open choice. Steps of another choice are not of this game.
        const update = latest.current;
        const facts = update?.tactical;
        const nonce = update?.pending_choice?.nonce;
        if (!facts || facts.kind === "activation" || !nonce) {
          return;
        }
        const place = facts.kind === "production" ? facts.system : facts.active;
        const saved = parseSaved(value);
        if (saved && (saved.nonce !== nonce || saved.destination !== place)) {
          return;
        }
        const steps = saved?.steps ?? [];
        setLocal((now) => {
          if (latest.current?.pending_choice?.nonce !== nonce || now.sent === nonce) {
            return now;
          }
          const mine = stagedSteps(update, now);
          if (sameStaged(mine ?? [], steps)) {
            return now;
          }
          return facts.kind === "movement"
            ? { ...now, movement: draftOf(facts, steps), planNote: null }
            : facts.kind === "invasion"
              ? { ...now, landing: landingOf(facts, steps), planNote: null }
              : { ...now, production: productionOf(facts, steps), planNote: null };
        });
      }),
    [drafts],
  );

  // The staged movement is kept for a reload, with the choice that it belongs to. So are the
  // staged landings and the staged production.
  useEffect(() => {
    const facts = update?.tactical;
    const nonce = update?.pending_choice?.nonce;
    if (!update || !drafts || !facts || facts.kind === "activation" || !nonce) {
      return;
    }
    const destination = facts.kind === "production" ? facts.system : facts.active;
    const now = { nonce, destination, steps: stagedSteps(update, local) };
    const before = shown.current.movement as typeof now | undefined;
    shown.current.movement = now;
    if (!before || sameStaged(before, now)) {
      return;
    }
    if (now.steps) {
      if (!sameStaged(now, drafts.get("movement"))) {
        drafts.set("movement", now);
      }
    } else if (before.nonce === nonce && before.steps && readDraft(drafts)?.nonce === nonce) {
      // What this tab staged for this choice is taken back. Another tab may have staged for a
      // choice that this tab is not told yet; what is kept for another choice is not read.
      drafts.set("movement", null);
    }
  }, [update, local, drafts]);

  // The private draft is kept for a reload, with its undo history.
  useEffect(() => {
    if (!drafts) {
      return;
    }
    const kept = keptPlan(plan.draft);
    const before = shown.current.plan;
    shown.current.plan = kept;
    if (
      before !== undefined &&
      !sameStaged(before, kept) &&
      !sameStaged(kept, drafts.get("plan"))
    ) {
      drafts.set("plan", kept);
    }
  }, [plan.draft, drafts]);

  // The secondary of the strategic action that is resolved can be drafted from the moment its
  // card is played, in Live: the row of the viewer in the list of the seats is the draft.
  const action = update?.secondary ?? null;
  const usable =
    !!update && !local.error && !local.replaying && !local.offline && !update.view.finished;
  const drafting = usable && !plan.open && action?.draft ? action : null;
  /** The draft of this action. A draft of an action that is over is dropped. */
  const mine =
    follow && update && action && isOf(follow, update.view.round, action) ? follow : null;
  useEffect(() => {
    if (update && follow && !(action?.draft && isOf(follow, update.view.round, action))) {
      setFollow(null);
    }
  }, [update, follow, action]);
  useEffect(() => {
    if (!drafts) {
      return;
    }
    const before = shown.current.secondary;
    shown.current.secondary = follow;
    if (
      before !== undefined &&
      !sameStaged(before, follow) &&
      !sameStaged(follow, drafts.get("secondary"))
    ) {
      drafts.set("secondary", follow);
    }
  }, [follow, drafts]);
  /** The nonce of the first question of the secondary, when the live game asks the viewer for it. */
  const reached =
    drafting &&
    update?.pending_choice &&
    update.pending_choice.nonce !== local.sent &&
    asksSecondary(update.pending_choice.choice, drafting)
      ? update.pending_choice.nonce
      : null;
  // The game is asked for the draft while its editor is in view, and before a draft is sent.
  const followWanted =
    !!drafting && (!update?.pending_choice || (reached !== null && !!mine?.answers.length));
  const scopePrimary = drafting?.primary ?? null;
  const scopeCard = drafting?.card ?? null;
  const followScope = useMemo(
    () =>
      scopePrimary !== null && scopeCard !== null
        ? scopeOf({ primary: scopePrimary, card: scopeCard })
        : null,
    [scopePrimary, scopeCard],
  );
  const followScript = useMemo(() => followScriptOf(mine), [mine]);

  // The draft is played again on a copy of the game, for each change of it and of the game. The
  // transport is asked, and answers later (`draft` of its events).
  const entry = plan.draft ? entryOf(plan.draft) : null;
  const locked = update ? draftLock(update, local) : "The game has not started";
  const script = useMemo(() => (entry ? scriptOf(entry) : null), [entry]);
  const wanted = useMemo(
    (): Wanted | null =>
      plan.open && !locked
        ? script && { script }
        : followWanted && followScope
          ? { script: followScript, scope: followScope }
          : null,
    [plan.open, locked, script, followWanted, followScope, followScript],
  );
  useEffect(() => {
    if (!transport || !update || !wanted) {
      asked.current = null;
      return setRun(null);
    }
    asked.current = { wanted, base: update };
    transport.draft(wanted.script, wanted.scope);
  }, [transport, update, wanted]);
  /**
   * The answer is of this script. The game may have changed since (another seat acted): the
   * screen and its main button stay as they are, and the answer for the new game replaces it.
   */
  const settled =
    !!run &&
    !!wanted &&
    (run.wanted === wanted ||
      (same(run.wanted.script, wanted.script) &&
        same(run.wanted.scope ?? null, wanted.scope ?? null)));
  const tactical = !!wanted && !wanted.scope;
  // Before the first answer the draft is open with nothing to decide, so that nothing of the
  // live game can be sent from it.
  const unanswered = useMemo((): DraftOutcome | null => {
    if (!update) {
      return null;
    }
    const { pending_choice: _, tactical: __, ...rest } = update;
    return { update: rest, consumed: 0, stop: { kind: "open" } };
  }, [update]);
  const outcome = !tactical
    ? null
    : settled
      ? run.outcome
      : ((run && !run.wanted.scope ? run.outcome : null) ?? unanswered);
  const waiting = tactical && !settled;
  /** What the game made of the draft of the secondary. Until the answer, of the draft before. */
  const followOutcome = wanted?.scope && run?.wanted.scope ? run.outcome : null;
  const followChecked = !!wanted?.scope && settled;

  // A ready draft is sent by the game when it asks the seat (`ti4_host::staged::ready`), once
  // for every tab of the seat, also when none is open: the page sends none. One that no longer
  // fits is written back not ready, and the page says so (`SeatStore.subscribe`).

  // The sheet of the landings or of the build opens when the draft comes to that step.
  const draftStage =
    entry && outcome && entry.system
      ? plan.stage && outcome[plan.stage]
        ? plan.stage
        : frontier(outcome)
      : null;
  const draftSheet =
    draftStage === "invasion" || draftStage === "production"
      ? `${draftStage}:${entry?.system}`
      : draftStage;
  useEffect(() => {
    const system = draftSheet?.includes(":") ? draftSheet.split(":")[1] : null;
    setLocal((now) => ({ ...now, inspected: system, productionStage: "build" }));
  }, [draftSheet]);

  const built = useMemo((): { session: GameSession; odds: OddsRequest | null } | null => {
    if (!update || !transport) {
      return null;
    }
    const drafted0 = mine ?? (drafting ? newFollow(update.view.round, drafting) : null);
    const over = !!drafted0 && followChecked && !!followOutcome && isOver(drafted0, followOutcome);
    const followInfo: FollowInfo | null =
      drafting && drafted0
        ? {
            answers: drafted0.answers.map(({ label }) => label),
            ready: drafted0.ready,
            editing:
              !update.pending_choice &&
              !drafted0.ready &&
              followOutcome?.stop.kind === "open" &&
              !!followOutcome.update.pending_choice,
            waiting: !followChecked && !update.pending_choice,
            over,
            problem: followOutcome ? followProblem(drafted0, followOutcome) : null,
            end: followOutcome ? endOf(followOutcome) : null,
            canSend: reached !== null && !drafted0.ready && over && drafted0.answers.length > 0,
          }
        : null;
    const live: LocalState = {
      ...local,
      draftLocked: locked,
      draft: null,
      follow: followInfo,
      drafted: plan.draft ? { system: entryOf(plan.draft).system } : null,
    };
    const drafted =
      plan.draft && outcome
        ? draftView(update, live, plan.draft, outcome, plan.stage, waiting)
        : null;
    // The editor of the secondary asks the question of the draft, on the game as it is.
    const shown =
      // A draft is played on a copy of the game, which no table is told to: the seats are those of the game.
      (drafted && { ...drafted.update, seats: update.seats }) ??
      (followInfo?.editing
        ? {
            ...update,
            pending_choice: followOutcome?.update.pending_choice,
            research: followOutcome?.update.research,
          }
        : update);
    const state = drafted?.local ?? live;
    // Nothing is sent while the game cannot be reached: the next update says what is open.
    const pending =
      state.sent === shown.pending_choice?.nonce || state.offline ? null : shown.pending_choice;
    /** Opens or closes the draft. What was staged for the other workspace is dropped. */
    const workspace = (open: boolean, draft: Draft | null) => {
      setPlan({ open, draft, stage: null });
      setLocal((now) => ({
        ...now,
        staged: null,
        pools: null,
        purchase: null,
        ready: null,
        inspected: null,
        handled: [],
        step: null,
      }));
    };
    /** A change of the draft: one step of its undo. */
    const redraft = (change: (now: Draft) => Draft) =>
      setPlan((now) => (now.draft ? { ...now, draft: change(now.draft) } : now));
    /**
     * A change of the step in view: one step of undo. The step stays in view, also when the
     * game comes to a later step with it.
     */
    const restage = (change: (now: Draft) => Draft) =>
      setPlan((now) =>
        now.draft ? { ...now, draft: change(now.draft), stage: drafted?.stage ?? now.stage } : now,
      );
    const stage = (option: string) => {
      if (pending?.choice.options.some((item) => item.id === option)) {
        setLocal((now) => ({ ...now, staged: now.staged === option ? null : option }));
      }
    };
    /**
     * A click on a system of the choice: it is staged, and it shows what is in it. Closing the
     * inspector does not take the choice back.
     */
    const inspectAndStage = (system: string) => {
      stage(system);
      setLocal((now) => ({ ...now, inspected: system }));
    };
    const facts = tacticalFacts(shown, state);
    const moving = facts?.kind === "movement" ? facts : null;
    const landing = facts?.kind === "invasion" ? facts : null;
    const producing = facts?.kind === "production" ? facts : null;
    /** A gain or a purchase of command tokens of the live game: one screen, sent as one plan. */
    const buying =
      pending && !drafted && !followInfo?.editing && selectPurchase(shown, pending.choice, null)
        ? pending
        : null;
    /** A readying of planets of the live game, with its system or its window: answered at once. */
    const readying =
      pending && !drafted && !followInfo?.editing && selectReady(shown, pending.choice, null)
        ? pending
        : null;
    /** The game can be asked for a staged production: it is paid, and within its limits. */
    const paidFor = (staged: ProductionDraft) => {
      const { units, cost, paid } = producing
        ? totals(producing, staged)
        : { units: 0, cost: 0, paid: 0 };
      return units === 0 || (paid >= cost && !!producing && overrunOf(producing, staged) === 0);
    };
    /** What a staged production sends: nothing is paid for nothing. */
    const productionPlan = (staged: ProductionDraft) => {
      const { units, cost } = producing ? totals(producing, staged) : { units: 0, cost: 0 };
      return units === 0 ? NO_PRODUCTION : cost === 0 ? clearPayment(staged) : staged;
    };
    /**
     * A change of the staged production. Live, it clears the note of the last plan. A draft
     * records it at once; the game is asked for it when it is paid.
     */
    const produce = (change: (now: ProductionDraft) => ProductionDraft) => {
      if (!drafted) {
        return setLocal((now) => ({ ...now, production: change(now.production), planNote: null }));
      }
      if (producing) {
        const staged = change(state.production);
        const ready = paidFor(staged);
        // A build that is not paid keeps its payment as it is staged.
        const steps = productionSteps(producing, ready ? productionPlan(staged) : staged);
        restage((now) => setProduction(now, steps, ready));
      }
    };
    /** The build is chosen: the payment is next, when there is something to pay. */
    const toPayment = () => {
      const { units, cost } = producing
        ? totals(producing, state.production)
        : { units: 0, cost: 0 };
      // A build over a limit stays open: the gauges and the footer say what is over.
      if (producing && overrunOf(producing, state.production) > 0) {
        return true;
      }
      const pays = units > 0 && cost > 0;
      setLocal((now) => ({ ...now, inspected: null, productionStage: pays ? "pay" : "build" }));
      return pays;
    };
    /** Back to the build of the system that produces: its sheet opens. */
    const toBuild = (system: string) =>
      setLocal((now) => ({ ...now, inspected: system, productionStage: "build" }));
    /**
     * A change of the staged landings. Live, it clears the note of the last plan. A draft
     * records it at once.
     */
    const land = (change: (now: LandingDraft) => LandingDraft) => {
      if (!drafted) {
        return setLocal((now) => ({ ...now, landing: change(now.landing), planNote: null }));
      }
      if (landing) {
        const steps = landingSteps(landing, change(state.landing));
        if (JSON.stringify(steps) !== JSON.stringify(landingSteps(landing, state.landing))) {
          restage((now) => setLandings(now, steps));
        }
      }
    };
    /** The step of the draft that is in view from now on. Null: where the draft is open. */
    const toStage = (stage: Stage | null) => {
      setPlan((now) => ({ ...now, stage }));
      setLocal((now) => ({ ...now, step: null, staged: null }));
    };
    /**
     * A change of the staged movement. Live, it clears the note of the last plan. A draft
     * records it at once, as the steps that the game is asked for.
     */
    const draft = (change: (now: MovementDraft) => MovementDraft) => {
      if (!drafted) {
        return setLocal((now) => ({ ...now, movement: change(now.movement), planNote: null }));
      }
      if (moving) {
        const steps = stepsOf(moving, change(state.movement));
        if (JSON.stringify(steps) !== JSON.stringify(stepsOf(moving, state.movement))) {
          restage((now) => setMovement(now, steps));
        }
      }
    };
    const unmark = (keep: (system: string) => boolean) =>
      setLocal((now) => ({ ...now, handled: now.handled.filter(keep) }));
    const send = () => {
      if (!pending) {
        return;
      }
      if (drafted && waiting) {
        // The screen is of the script before: its main button waits for the game.
        return;
      }
      // A research with nothing chosen in its tree researches nothing.
      const pass =
        shown.research && !state.staged
          ? (pending.choice.options.find((item) => item.kind === "decline")?.id ?? null)
          : null;
      // A redistribution of command tokens answers with the arrangement of its pools.
      const pools = selectTokens(pending.choice, state.pools ?? null);
      if (followInfo?.editing) {
        // Nothing of a draft is sent: the answer is recorded, and the game is asked again.
        const chosen = pools ? pools.staged : (state.staged ?? pass);
        const option = pending.choice.options.find((item) => item.id === chosen);
        if (option && drafted0 && followChecked) {
          setFollow(record(drafted0, { id: option.id, label: option.label || option.id }));
          setLocal((now) => ({ ...now, staged: null, pools: null }));
        }
        return;
      }
      if (drafted) {
        // Nothing of a draft is sent. Its activation is recorded, and a step with nothing
        // staged is recorded as that ("Move nothing"), so that the draft goes on.
        const system = facts?.kind === "activation" ? state.staged : null;
        if (system) {
          redraft((now) => activate(now, system));
          setLocal((now) => ({ ...now, staged: null, inspected: null }));
        } else if (moving) {
          redraft((now) => setMovement(now, stepsOf(moving, state.movement)));
          toStage(null);
        } else if (landing) {
          redraft((now) => setLandings(now, landingSteps(landing, state.landing)));
          toStage(null);
        } else if (producing) {
          if (totals(producing, state.production).units === 0) {
            redraft((now) => setProduction(now, productionSteps(producing, NO_PRODUCTION), true));
          } else if (local.productionStage === "build") {
            toPayment();
          }
        } else if (
          state.staged &&
          drafted.stage === null &&
          plan.draft &&
          entryOf(plan.draft).system
        ) {
          // An ability of the seat asks before a step: the answer is of the step that comes.
          const reached = drafted.local.draft?.reached ?? [];
          const before = STAGES[reached.length];
          const option = state.staged;
          if (before) {
            redraft((now) => answer(now, before, option));
            setLocal((now) => ({ ...now, staged: null }));
          }
        }
        return;
      }
      if (producing) {
        if (local.productionStage === "build" && toPayment()) {
          return;
        }
        if (!paidFor(local.production)) {
          return;
        }
        transport.submitPlan(pending.nonce, {
          destination: producing.system,
          steps: productionSteps(producing, productionPlan(local.production)),
        });
        return setLocal((now) => ({ ...now, sent: pending.nonce, planNote: null }));
      }
      if (moving || landing) {
        transport.submitPlan(
          pending.nonce,
          moving
            ? { destination: moving.active, steps: stepsOf(moving, local.movement) }
            : { destination: landing!.active, steps: landingSteps(landing!, local.landing) },
        );
        return setLocal((now) => ({ ...now, sent: pending.nonce, planNote: null }));
      }
      // A gain or a purchase of command tokens is sent as one plan, with its payment.
      if (buying) {
        const steps = purchasePlan(buying.choice, local.purchase ?? null);
        if (steps) {
          transport.submitPlan(pending.nonce, { destination: "", steps });
          setLocal((now) => ({ ...now, purchase: null, sent: pending.nonce, planNote: null }));
        }
        return;
      }
      // A readying of planets answers its system or its window and each planet, in this order.
      if (readying) {
        const answers = selectReady(shown, readying.choice, local.ready ?? null)?.answers;
        if (answers) {
          transport.applyDraft(pending.nonce, { choices: answers, plan: null });
          setLocal((now) => ({ ...now, ready: null, sent: pending.nonce, planNote: null }));
        }
        return;
      }
      const chosen = pools ? pools.staged : (local.staged ?? pass);
      if (chosen) {
        transport.submitChoice(pending.nonce, chosen);
        // The details that the choice of the system opened close with it: the movement starts
        // with the whole map in view, as in a draft.
        const opened = facts?.kind === "activation" ? local.staged : null;
        setLocal((now) => ({
          ...now,
          staged: null,
          sent: pending.nonce,
          inspected: opened !== null && now.inspected === opened ? null : now.inspected,
        }));
      }
    };
    const apply = () => {
      const nonce = menuNonce(update, live);
      const system = plan.draft ? entryOf(plan.draft).system : null;
      if (!drafted?.local.draft?.canApply || !plan.draft || nonce === null || system === null) {
        return;
      }
      // What the game played is sent. What the draft holds after a roll is staged again in Live,
      // when its step opens.
      const { sent } = drafted;
      const rest = (scriptOf(entryOf(plan.draft)).slice(2 + sent.length) as PlanStep[]).filter(
        (step) => stageOf(step) !== null,
      );
      transport.applyDraft(nonce, {
        choices: ["tactical", system],
        plan: sent.length ? { destination: system, steps: sent } : null,
      });
      workspace(false, null);
      setLocal((now) => ({
        ...now,
        sent: nonce,
        planNote: null,
        remaining: rest.length ? { destination: system, steps: rest, drafted: true } : null,
      }));
    };
    const dispatch = (intent: Intent) => {
      switch (intent.type) {
        case "workspace":
          if (intent.mode === "live") {
            return workspace(false, plan.draft);
          }
          return intent.mode === "draft" && !locked
            ? workspace(true, plan.draft ?? newDraft())
            : undefined;
        case "redo":
          return redraft(redo);
        case "startOver":
          // The draft of a secondary starts over with nothing answered.
          return drafted ? redraft(startOver) : followInfo ? setFollow(null) : undefined;
        case "discardDraft":
          return workspace(false, null);
        case "confirmApply":
          if (followInfo?.canSend && reached !== null && drafted0) {
            // The draft that was not marked ready is sent as it is.
            transport.applyDraft(reached, {
              choices: drafted0.answers.map(({ id }) => id),
              plan: null,
            });
            setFollow(null);
            return setLocal((now) => ({ ...now, sent: reached, planNote: null }));
          }
          return apply();
        case "simulate":
          return transport.capabilities.stepping?.step();
        case "inspectSystem":
          // The system that produces opens its build, also from the payment.
          return producing?.system === intent.system
            ? toBuild(intent.system)
            : setLocal((now) => ({ ...now, inspected: intent.system }));
        case "closeInspector":
          return setLocal((now) => ({ ...now, inspected: null }));
        case "pickAction": {
          if (intent.action !== "tactical") {
            return intent.card ? stage(intent.card) : undefined;
          }
          // A tactical action opens as the private draft: the one that the seat prepared, or a
          // new one. A system of the map is staged in it; a draft of another system starts over,
          // which is one step of its undo.
          if (locked || !pending?.choice.options.some((option) => option.id === "tactical")) {
            return;
          }
          const kept = plan.draft ?? newDraft();
          const system = intent.system ?? null;
          const other = system !== null && entryOf(kept).system !== null;
          const same = other && entryOf(kept).system === system;
          workspace(true, other && !same ? startOver(kept) : kept);
          if (system !== null && !same) {
            setLocal((now) => ({ ...now, staged: system, inspected: system }));
          }
          return;
        }
        case "pickGroup":
          return setLocal((now) => ({ ...now, menu: intent.group, staged: null }));
        case "backToPicker":
          // One level back: from the draft to the turn menu, from a staged action to its list,
          // from a list to the picker.
          if (plan.open) {
            return workspace(false, plan.draft);
          }
          return setLocal((now) =>
            now.staged ? { ...now, staged: null } : { ...now, menu: null },
          );
        case "pass":
          return stage("pass");
        case "endTurn": {
          const end = pending?.choice.options.find(
            (option) => option.kind === "end_turn" || option.id === "end_turn",
          );
          if (!pending || !end || drafted) {
            return;
          }
          transport.submitChoice(pending.nonce, end.id);
          return setLocal((now) => ({ ...now, staged: null, sent: pending.nonce }));
        }
        case "pickSeat": {
          const option = optionOfSeat(pending?.choice ?? null, intent.seat);
          return option ? stage(option) : undefined;
        }
        case "chooseOption": {
          // The kind of ground force that a tap lands is a choice of the hand.
          const force = intent.group === "landing" ? forceOfId(intent.option) : null;
          return landing && force !== null
            ? setLocal((now) => ({ ...now, landingKind: force }))
            : stage(intent.option);
        }
        case "togglePlanet": {
          // A planet of the active system: one more unit of the chosen kind lands there.
          const { planet } = intent;
          if (producing) {
            // A planet pays, once the build is chosen.
            return (
              local.productionStage === "pay" &&
              produce((now) => togglePlanet(producing, now, planet))
            );
          }
          if (buying) {
            // A planet pays for the bought command tokens.
            const paying = togglePurchasePlanet(buying.choice, local.purchase ?? null, planet);
            return paying && setLocal((now) => ({ ...now, purchase: paying }));
          }
          if (readying) {
            const readied = toggleReadyPlanet(readying.choice, local.ready ?? null, planet);
            return readied && setLocal((now) => ({ ...now, ready: readied }));
          }
          if (!landing) {
            // A planet of a choice on the board: its option is staged.
            const option = optionOnBoard(shown, pending?.choice ?? null, planet);
            return option ? stage(option) : undefined;
          }
          return (
            landing &&
            land((now) => {
              const force = chosenForce(landing, now, local.landingKind, planet);
              return force === null
                ? now
                : setLanding(landing, now, planet, force, (now[planet]?.[force] ?? 0) + 1);
            })
          );
        }
        case "setPayment":
          if (buying) {
            const goods = setPurchaseGoods(buying.choice, local.purchase ?? null, intent.value);
            return goods && setLocal((now) => ({ ...now, purchase: goods }));
          }
          return (
            producing &&
            intent.source === GOODS &&
            produce((now) => setGoods(producing, now, intent.value))
          );
        case "payment":
          if (buying) {
            const paid = payPurchase(buying.choice, local.purchase ?? null, intent.action);
            return paid && setLocal((now) => ({ ...now, purchase: paid }));
          }
          return (
            producing &&
            produce((now) =>
              intent.action === "auto" ? autoPay(producing, now) : clearPayment(now),
            )
          );
        case "toggleSystem": {
          // The system of a readying of planets: it is chosen, and it shows what is in it.
          const chosen =
            readying && chooseReadySystem(readying.choice, local.ready ?? null, intent.system);
          if (chosen) {
            return setLocal((now) => ({ ...now, ready: chosen, inspected: intent.system }));
          }
          // In a movement the fleet of a system opens at the system, and closes there. So does
          // the build of a production.
          return moving || producing
            ? setLocal((now) => ({
                ...now,
                inspected: now.inspected === intent.system ? null : intent.system,
              }))
            : inspectAndStage(
                optionOnBoard(shown, pending?.choice ?? null, intent.system) ?? intent.system,
              );
        }
        case "clearPart": {
          const cleared =
            readying && clearReadyPart(readying.choice, local.ready ?? null, intent.part);
          return cleared && setLocal((now) => ({ ...now, ready: cleared }));
        }
        case "findSystem": {
          const found = pending?.choice.options.find((item) => sameSystem(intent.query, item.id));
          if (found && facts?.kind === "activation") {
            setLocal((now) => ({ ...now, staged: found.id, inspected: found.id }));
          }
          return;
        }
        case "clearChoice": {
          // Esc in the payment of a production goes back to its build.
          if (producing && local.productionStage === "pay") {
            return toBuild(producing.system);
          }
          // Esc in a readying of planets clears the last choice.
          const back = readying && clearLastReady(readying.choice, local.ready ?? null);
          if (back) {
            return setLocal((now) => ({ ...now, ready: back }));
          }
          return setLocal((now) => ({
            ...now,
            staged: null,
            pools: null,
            purchase: null,
            ready: null,
          }));
        }
        case "undo":
          if (drafted) {
            return redraft(undo);
          }
          return transport.undo();
        case "flow":
          if (intent.action === "ready") {
            return followInfo?.over && !followInfo.problem && drafted0
              ? setFollow({ ...drafted0, ready: true })
              : undefined;
          }
          if (intent.action === "change") {
            return mine ? setFollow({ ...mine, ready: false }) : undefined;
          }
          return intent.action === "resolve" && !facts ? send() : undefined;
        // The steps of a tactical action.
        case "commitEdit":
          return facts ? send() : undefined;
        case "selectStep": {
          // A step of a draft that the game came to is opened to change it.
          const stage = ([null, "movement", null, "invasion", "production"] as const)[intent.step];
          if (drafted && stage) {
            return toStage(stage);
          }
          if (drafted && intent.step === 0 && !moving) {
            setPlan((now) => ({ ...now, stage: "movement" }));
            return setLocal((now) => ({ ...now, step: 0 }));
          }
          return setLocal((now) => ({ ...now, step: moving && intent.step === 0 ? 0 : null }));
        }
        case "returnToCurrent":
          return setLocal((now) => ({ ...now, step: null }));
        case "setCount":
          if (producing) {
            return produce((now) => setProductionCount(producing, now, intent.key, intent.value));
          }
          if (landing) {
            return land((now) => setLandingCount(landing, now, intent.key, intent.value));
          }
          return moving && draft((now) => setCount(moving, now, intent.key, intent.value));
        case "fillHold": {
          const pool = poolOfSource(intent.source);
          const origin = intent.key.startsWith("origin:") ? intent.key.slice(7) : null;
          return (
            moving &&
            pool !== null &&
            origin !== null &&
            draft((now) => fill(moving, now, origin, pool))
          );
        }
        case "resetOrigin":
          if (producing) {
            return produce(() => NO_PRODUCTION);
          }
          if (landing) {
            return land(() => ({}));
          }
          unmark((system) => system !== intent.system);
          return moving && draft((now) => resetOrigin(now, intent.system));
        case "markOrigin":
          if (producing) {
            return void toPayment();
          }
          if (landing) {
            return setLocal((now) => ({ ...now, inspected: null }));
          }
          return (
            moving &&
            setLocal((now) => ({
              ...now,
              inspected: null,
              handled: [...new Set([...now.handled, intent.system])],
            }))
          );
        case "cancelEdit":
          if (producing) {
            produce(() => NO_PRODUCTION);
            return setLocal((now) => ({ ...now, productionStage: "build" }));
          }
          if (landing) {
            return land(() => ({}));
          }
          unmark(() => false);
          return moving && draft(() => ({}));
        case "setOffer": {
          const { name, mode } = intent;
          transport.setReactionMode(intent.key, mode);
          return setToast((now) => ({
            id: (now?.id ?? 0) + 1,
            text:
              mode === "never"
                ? `${name}: never offered. Its windows are passed and you are told.`
                : mode === "ownturn"
                  ? `${name}: offered on your turns only. Its other windows are passed and you are told.`
                  : `${name}: offered again.`,
          }));
        }
        case "reaction": {
          // The main button of a reaction window: pass, or use what is staged.
          if (intent.action === "play") {
            return send();
          }
          const decline = shown.reaction
            ? pending?.choice.options.find((option) => option.kind === "decline")
            : undefined;
          if (!pending || !decline || drafted || followInfo?.editing) {
            return;
          }
          transport.submitChoice(pending.nonce, decline.id);
          return setLocal((now) => ({ ...now, staged: null, sent: pending.nonce }));
        }
        case "flowCount": {
          if (buying) {
            const bought = countPurchase(
              buying.choice,
              local.purchase ?? null,
              intent.key,
              intent.value,
            );
            return bought && setLocal((now) => ({ ...now, purchase: bought }));
          }
          // A counter of a pool of command tokens: the arrangement is staged when it is offered.
          const counted =
            pending && countPools(pending.choice, state.pools ?? null, intent.key, intent.value);
          return (
            counted && setLocal((now) => ({ ...now, pools: counted.pools, staged: counted.staged }))
          );
        }
        case "setCardFake": {
          const { card } = intent;
          return setFaked((now) => [
            ...now.filter((name) => name !== card),
            ...(intent.fake ? [card] : []),
          ]);
        }
        default:
          // The other intents belong to screens that local play does not have yet.
          return;
      }
    };
    const { stepping, switchSeat, scenarios, save } = transport.capabilities;
    return {
      session: {
        view: { ...selectShell(shown, state, transport.catalog, cards), toast },
        dispatch,
        capabilities: {
          stepping: !!stepping,
          switchSeat: !!switchSeat,
          scenarios: !!scenarios,
          save: !!save,
        },
      },
      // The odds of the space combat that the movement in view starts, Live or in a draft.
      odds: moving && local.step === null ? oddsRequest(moving, state.movement) : null,
    };
  }, [
    update,
    local,
    transport,
    plan,
    outcome,
    waiting,
    locked,
    cards,
    toast,
    drafting,
    mine,
    reached,
    followOutcome,
    followChecked,
  ]);

  // The engine of the page is asked for the odds of each new movement. The answer for a movement
  // that changed since is not kept: the answer for the new one is on its way.
  const oddsKey = built?.odds?.key ?? null;
  useEffect(() => {
    if (!transport || oddsKey === null) {
      return;
    }
    let current = true;
    const keep = (response: KeptOdds["response"]) =>
      current && setLocal((now) => ({ ...now, odds: { key: oddsKey, response } }));
    transport.odds(JSON.parse(oddsKey)).then(keep, () => keep(null));
    return () => {
      current = false;
    };
  }, [transport, oddsKey]);

  return { session: built?.session ?? null, error: local.error, replaying: local.replaying };
}
