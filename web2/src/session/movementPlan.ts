// A staged movement as the steps that the game is asked for, and back. The steps name what is
// moved and loaded, not the ids of the options: the game numbers its options again after each ship.
// How a step answers a question of the game is decided in the engine (`plan_answer` in
// `crates/ti4-view/src/planning/steps.rs`), for a draft and for a plan that is sent.
import { type MovementDraft, canMove, setLoad, setShip, shipId, shipOf } from "./movementDraft";
import type { ChoiceOption, MovementFacts } from "./wire";

/** Compare `MovementOnlyStep` in `crates/ti4-server/src/session/batch.rs`. */
export type MovementStep =
  | {
      kind: "move";
      origin: string;
      unit: string;
      damaged: boolean;
      gravity_drive: boolean;
      ionian: boolean;
    }
  | {
      kind: "load";
      /** The system that the ship starts in. */
      origin: string;
      /** The system where the unit is picked up. */
      pickup_system: string;
      unit: string;
      /** The planet, or null for the space area. */
      source: string | null;
      damaged: boolean;
      galvanized: boolean;
    }
  | { kind: "done_loading" }
  | { kind: "done_moving" };

/** The landings of an invasion, one unit at a time. The game asks for them as it asks for ships. */
export type LandingStep =
  | {
      kind: "land";
      planet: string;
      unit: string;
      damaged: boolean;
      /** Where the unit is, when it is not the space area of the active system. */
      from_system: string | null;
      from_planet: string | null;
    }
  | { kind: "done_committing" };

/**
 * One use of PRODUCTION. The game asks for a unit, then for what pays it, then for its place
 * when it has more than one, and then for the next unit.
 */
export type ProductionStep =
  /** One purchase of a unit: two fighters or infantry, one of any other unit. */
  | { kind: "produce"; unit: string }
  /** Where the units of the purchase before it go: `space`, or a planet. */
  | { kind: "place"; destination: string }
  /** The id of the option that pays: "exhaust|<planet>" or "trade_good". */
  | { kind: "pay"; source: string }
  | { kind: "done_producing" };

/** An answer to what the game asks between the steps (an ability of the seat), by the id of the option. */
export type ChooseStep = { kind: "choose"; option_id: string };

export type PlanStep = MovementStep | LandingStep | ProductionStep | ChooseStep;

/** The step of a tactical action that a step of a plan belongs to. Null for an answer by id. */
export const stageOf = (step: PlanStep): "movement" | "invasion" | "production" | null =>
  step.kind === "choose"
    ? null
    : step.kind === "move" ||
        step.kind === "load" ||
        step.kind === "done_loading" ||
        step.kind === "done_moving"
      ? "movement"
      : step.kind === "land" || step.kind === "done_committing"
        ? "invasion"
        : "production";

/**
 * The steps of a tactical action: its movement, its landings or its production, or several of
 * them in this order when a draft is sent.
 */
export interface MovementPlan {
  /** The active system. */
  destination: string;
  steps: PlanStep[];
}

/** The steps of a staged movement: each ship with its hold, in the order of the facts. */
export function stepsOf(facts: MovementFacts, draft: MovementDraft): MovementStep[] {
  const steps: MovementStep[] = [];
  for (const ship of facts.ships) {
    const hold = draft[shipId(ship)];
    if (!hold || !ship.move) {
      continue;
    }
    steps.push({
      kind: "move",
      origin: ship.origin,
      unit: ship.unit,
      damaged: ship.damaged,
      gravity_drive: ship.move.gravity_drive,
      ionian: ship.move.ionian,
    });
    for (const [pool, count] of Object.entries(hold)) {
      const from = facts.cargo[Number(pool)];
      for (let loaded = 0; from && loaded < count; loaded++) {
        steps.push({
          kind: "load",
          origin: ship.origin,
          pickup_system: from.system,
          unit: from.unit,
          source: from.source,
          damaged: from.damaged,
          galvanized: from.galvanized,
        });
      }
    }
    if (ship.capacity > 0) {
      steps.push({ kind: "done_loading" });
    }
  }
  steps.push({ kind: "done_moving" });
  return steps;
}

/**
 * The steps that a stopped plan did not reach, staged again on the facts of now. A step that
 * no longer fits is left out: the player sees what is staged before it is sent again.
 */
export function draftOf(facts: MovementFacts, steps: PlanStep[]): MovementDraft {
  let draft: MovementDraft = {};
  let open: string | null = null;
  for (const step of steps) {
    if (step.kind === "move") {
      const ship = facts.ships.find(
        (item) =>
          !(shipId(item) in draft) &&
          item.origin === step.origin &&
          item.unit === step.unit &&
          item.damaged === step.damaged &&
          canMove(facts, draft, item),
      );
      open = ship ? shipId(ship) : null;
      draft = open ? setShip(facts, draft, open, true) : draft;
    } else if (step.kind === "load" && open) {
      const pool = facts.cargo.findIndex(
        (item) =>
          item.system === step.pickup_system &&
          item.source === step.source &&
          item.unit === step.unit &&
          item.damaged === step.damaged &&
          item.galvanized === step.galvanized,
      );
      if (pool >= 0 && shipOf(facts, open)) {
        draft = setLoad(facts, draft, open, pool, (draft[open]?.[pool] ?? 0) + 1);
      }
    }
  }
  return draft;
}

/**
 * The step that an option of a movement is. Null for any other option. The inverse of
 * `PlanStep::matches_option` in `crates/ti4-view/src/planning/steps.rs`.
 */
export function stepOf(option: Pick<ChoiceOption, "id" | "kind" | "payload">): PlanStep | null {
  const { payload } = option;
  const on = (key: string) => payload[key] === true;
  if (option.kind === "move") {
    return {
      kind: "move",
      origin: String(payload.origin),
      unit: String(payload.unit),
      damaged: on("damaged"),
      gravity_drive: on("gravity_drive"),
      ionian: on("ionian"),
    };
  }
  if (option.kind === "load") {
    return {
      kind: "load",
      origin: String(payload.system),
      pickup_system: String(payload.pickup_system),
      unit: String(payload.unit),
      source: typeof payload.source === "string" ? payload.source : null,
      damaged: on("damaged"),
      galvanized: on("galvanized"),
    };
  }
  if (option.kind === "commit") {
    const place = (key: string) => (typeof payload[key] === "string" ? String(payload[key]) : null);
    return {
      kind: "land",
      planet: String(payload.planet),
      unit: String(payload.unit),
      damaged: on("damaged"),
      from_system: place("from_system"),
      from_planet: place("from_planet"),
    };
  }
  if (option.kind === "produce" && payload.exchange !== true) {
    return { kind: "produce", unit: String(payload.unit) };
  }
  if (option.kind === "place" && typeof payload.destination === "string") {
    return { kind: "place", destination: payload.destination };
  }
  if (option.kind === "pay") {
    return { kind: "pay", source: option.id };
  }
  return option.id === "done_loading" ||
    option.id === "done_moving" ||
    option.id === "done_committing" ||
    option.id === "done_producing"
    ? { kind: option.id }
    : null;
}
