// The private draft of a tactical action: what the player chose so far, and every state it had,
// for undo and redo. A draft is a script. The engine plays it on a copy of the game each time it
// changes (`Transport.draft`), so nothing here decides a rule.
import type { LandingStep, MovementStep, PlanStep, ProductionStep } from "./movementPlan";
import type { DraftOutcome, DraftStep, SessionUpdate } from "./wire";

/** The steps of a tactical action that a draft stages, in their order. */
export type Stage = "movement" | "invasion" | "production";
export const STAGES: Stage[] = ["movement", "invasion", "production"];

/** One state of the draft. A step that is null is not drafted: it is decided in Live. */
export interface DraftEntry {
  /** The system that the action activates. Null before it is chosen. */
  system: string | null;
  /** The movement, as "Move fleet" would send it. Null while nothing of it was changed. */
  steps: MovementStep[] | null;
  /** The landings, as "Commit ground forces" would send them. */
  landings?: LandingStep[] | null;
  /** The production with its payment, as "Produce units" would send it. */
  production?: ProductionStep[] | null;
  /** The production is not paid, or over a limit: it is kept, and the game is not asked for it. */
  unpaid?: boolean;
  /** What the game asks before a step (an ability of the seat), answered by the ids of the options. */
  answers?: Partial<Record<Stage, string[]>>;
}

export interface Draft {
  /** Every state of the draft, oldest first. Never empty. */
  history: DraftEntry[];
  /** The state that is the draft now. */
  cursor: number;
}

const EMPTY: DraftEntry = { system: null, steps: null };

export const newDraft = (): Draft => ({ history: [EMPTY], cursor: 0 });

export const entryOf = (draft: Draft): DraftEntry => draft.history[draft.cursor] ?? EMPTY;

/** A change is one step of undo. It drops what redo would have brought back. */
const record = (draft: Draft, entry: DraftEntry): Draft => {
  const history = [...draft.history.slice(0, draft.cursor + 1), entry];
  return { history, cursor: history.length - 1 };
};

export const activate = (draft: Draft, system: string): Draft =>
  record(draft, { system, steps: null });

/**
 * Another system for the recorded activation. The steps after it stay, and the game checks them
 * again: what no longer fits is marked where it is.
 */
export const reactivate = (draft: Draft, system: string): Draft =>
  entryOf(draft).system === system ? draft : record(draft, { ...entryOf(draft), system });

// A change of a step keeps the steps after it: the game checks them again.
export const setMovement = (draft: Draft, steps: MovementStep[]): Draft =>
  record(draft, { ...entryOf(draft), steps });

export const setLandings = (draft: Draft, landings: LandingStep[]): Draft =>
  record(draft, { ...entryOf(draft), landings });

/** `ready`: the build is paid and within its limits, so the game can be asked for it. */
export const setProduction = (draft: Draft, production: ProductionStep[], ready: boolean): Draft =>
  record(draft, { ...entryOf(draft), production, unpaid: !ready });

/** An answer to what the game asks before `stage`. */
export const answer = (draft: Draft, stage: Stage, option: string): Draft => {
  const entry = entryOf(draft);
  const answers = { ...entry.answers, [stage]: [...(entry.answers?.[stage] ?? []), option] };
  return record(draft, { ...entry, answers });
};

/** The steps of one stage, as they are recorded. Null when the stage is not drafted. */
export const stepsOfStage = (entry: DraftEntry, stage: Stage): PlanStep[] | null =>
  (stage === "movement" ? entry.steps : stage === "invasion" ? entry.landings : entry.production) ??
  null;

export const startOver = (draft: Draft): Draft =>
  entryOf(draft).system === null ? draft : record(draft, EMPTY);

export const canUndo = (draft: Draft) => draft.cursor > 0;
export const canRedo = (draft: Draft) => draft.cursor < draft.history.length - 1;
export const undo = (draft: Draft): Draft =>
  canUndo(draft) ? { ...draft, cursor: draft.cursor - 1 } : draft;
export const redo = (draft: Draft): Draft =>
  canRedo(draft) ? { ...draft, cursor: draft.cursor + 1 } : draft;

/** How many steps of a script come before the movement: the action and its system. */
export const BEFORE_MOVEMENT = 2;

/**
 * What the engine is asked to play: the tactical action, its system, then each drafted step
 * with the answers that the game asks for before it. A step that is not drafted adds nothing:
 * the run is open at its first question.
 */
export function scriptOf(entry: DraftEntry): DraftStep[] {
  const choose = (option_id: string): DraftStep => ({ kind: "choose", option_id });
  if (entry.system === null) {
    return [choose("tactical")];
  }
  return [
    choose("tactical"),
    choose(entry.system),
    ...STAGES.flatMap((stage) => [
      ...(entry.answers?.[stage] ?? []).map(choose),
      ...(stage === "production" && entry.unpaid ? [] : (stepsOfStage(entry, stage) ?? [])),
    ]),
  ];
}

/** The stage that entry `index` of the script of `entry` belongs to. Null for the action and its system. */
export function stageAt(entry: DraftEntry, index: number): Stage | null {
  let at = BEFORE_MOVEMENT;
  if (index < at) {
    return null;
  }
  for (const stage of STAGES) {
    at += entry.answers?.[stage]?.length ?? 0;
    at += stage === "production" && entry.unpaid ? 0 : (stepsOfStage(entry, stage)?.length ?? 0);
    if (index < at) {
      return stage;
    }
  }
  return "production";
}

/** The game played the whole script: the draft can be sent as it is. */
export const fits = (entry: DraftEntry, outcome: DraftOutcome): boolean =>
  (outcome.stop.kind === "open" || outcome.stop.kind === "complete") &&
  outcome.consumed === scriptOf(entry).length;

/** The first question of a stage in a run, with its facts. Undefined when the run did not ask it. */
export const stageUpdate = (outcome: DraftOutcome, stage: Stage): SessionUpdate | undefined =>
  outcome[stage];

const SUBTYPES: Record<string, Stage> = {
  movement_step: "movement",
  commit_ground_forces: "invasion",
  produce_unit: "production",
};

/** The stage whose first question is open at the end of a run. Null for any other end. */
export const openStage = (outcome: DraftOutcome): Stage | null =>
  (outcome.stop.kind === "open" &&
    SUBTYPES[outcome.update.pending_choice?.choice.context?.subtype ?? ""]) ||
  null;

/**
 * The stage that a draft shows when the player chose none: where the run is open, or the last
 * stage that it reached. Null: the run is at a question that is no stage (the activation, an
 * ability of the seat).
 */
export function frontier(outcome: DraftOutcome): Stage | null {
  if (outcome.stop.kind === "open") {
    return openStage(outcome);
  }
  return STAGES.findLast((stage) => outcome[stage] !== undefined) ?? null;
}

/** The stage that the run reached after `stage`. Null when it reached none. */
export const stageAfter = (outcome: DraftOutcome, stage: Stage): Stage | null =>
  STAGES.slice(STAGES.indexOf(stage) + 1).find((later) => outcome[later] !== undefined) ?? null;

const STOPPED: Record<string, string> = {
  Uncertainty: "The draft ends at a roll or a draw. Apply to go on in Live.",
  UnsupportedSegment: "The draft ends before the space combat. Apply to go on in Live.",
  OtherPlayerRequired: "The draft ends where another seat decides.",
  KnowledgeChanged: "The draft ends where the game would show something new.",
  StepLimit: "The draft is too long to check.",
};

/** Why a run stopped at something a draft cannot show, in words. */
export const stopText = (reason: string): string =>
  STOPPED[reason] ?? "The draft ends at a decision that it cannot show yet.";

const PARTS: Record<Stage, string> = {
  movement: "movement",
  invasion: "landings",
  production: "production",
};

/** A stop that ends the check of a draft, not the draft: the game goes on from there in Live. */
export const isBoundary = (outcome: DraftOutcome): boolean =>
  outcome.stop.kind === "stopped" &&
  (outcome.stop.reason === "Uncertainty" || outcome.stop.reason === "UnsupportedSegment");

/** Why the game did not play the whole script, in words. Null when it did. */
export function problemOf(entry: DraftEntry, outcome: DraftOutcome): string | null {
  const { stop } = outcome;
  switch (stop.kind) {
    case "open":
    case "complete":
      return null;
    case "refused": {
      const stage = stageAt(entry, stop.step);
      return stop.step === 0
        ? "A tactical action cannot be drafted from here: the game asks something else first."
        : stage === null
          ? `System ${entry.system} cannot be activated now.`
          : `A part of the ${PARTS[stage]} no longer fits. ${stop.reason}`;
    }
    case "stopped":
      return stopText(stop.reason);
    case "failed":
      return `No draft from here: ${stop.reason}.`;
  }
}

/** The draft as it is kept between two visits (`SeatStore`, slot "plan"). Null: none, or not a draft. */
export function parseDraft(value: unknown): Draft | null {
  const kept = value as Partial<Draft> | null;
  return kept &&
    Array.isArray(kept.history) &&
    kept.history.length > 0 &&
    Number.isInteger(kept.cursor) &&
    kept.cursor! >= 0 &&
    kept.cursor! < kept.history.length
    ? { history: kept.history, cursor: kept.cursor! }
    : null;
}

/**
 * The draft within `bytes` as JSON: the oldest states of its undo go first, then the states that
 * redo would bring back. The state that is the draft now stays.
 */
export function trimDraft(draft: Draft, bytes: number, size: (draft: Draft) => number): Draft {
  let trimmed = draft;
  while (size(trimmed) > bytes && trimmed.history.length > 1) {
    trimmed =
      trimmed.cursor > 0
        ? { history: trimmed.history.slice(1), cursor: trimmed.cursor - 1 }
        : { history: trimmed.history.slice(0, -1), cursor: 0 };
  }
  return trimmed;
}
