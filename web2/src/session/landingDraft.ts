// The staged landings of an invasion: how many units of each kind land on each planet. Nothing
// of it is sent before "Commit ground forces". What can land where, and how many units there
// are, are facts of the engine; this file only counts what is staged against them.
import type { LandingStep, PlanStep } from "./movementPlan";
import type { InvasionFacts } from "./wire";

/** For each planet: how many units of each force of the facts land there. */
export type LandingDraft = Record<string, Record<number, number>>;

/** How many units of a force are staged, on all planets. */
export const forceTaken = (draft: LandingDraft, force: number) =>
  Object.values(draft).reduce((sum, lands) => sum + (lands[force] ?? 0), 0);

export const forceLeft = (facts: InvasionFacts, draft: LandingDraft, force: number) =>
  (facts.forces[force]?.count ?? 0) - forceTaken(draft, force);

/** How many units land, on all planets. */
export const landed = (draft: LandingDraft) =>
  Object.values(draft).reduce(
    (sum, lands) => sum + Object.values(lands).reduce((part, count) => part + count, 0),
    0,
  );

/** Sets how many units of one force land on a planet, within what is left of the force. */
export function setLanding(
  facts: InvasionFacts,
  draft: LandingDraft,
  planet: string,
  force: number,
  count: number,
): LandingDraft {
  if (!facts.planets.find((item) => item.planet === planet)?.lands?.includes(force)) {
    return draft;
  }
  const now = draft[planet]?.[force] ?? 0;
  const next = Math.max(0, Math.min(count, now + forceLeft(facts, draft, force)));
  const { [force]: _, ...others } = draft[planet] ?? {};
  const lands = next ? { ...others, [force]: next } : others;
  const { [planet]: __, ...planets } = draft;
  return Object.keys(lands).length ? { ...planets, [planet]: lands } : planets;
}

// The ids of the staged values, as the sheet sends them back in `setCount` and `chooseOption`.
export const landKey = (planet: string, force: number) => `land:${planet}:${force}`;
export const forceId = (force: number) => `force:${force}`;
export const forceOfId = (id: string | undefined) =>
  id?.startsWith("force:") ? Number(id.slice(6)) : null;

/** A `setCount` of the landing sheet. Any other key leaves the draft as it is. */
export function setCount(
  facts: InvasionFacts,
  draft: LandingDraft,
  key: string,
  value: number,
): LandingDraft {
  const [, planet, force] = /^land:(.+):(\d+)$/.exec(key) ?? [];
  return planet ? setLanding(facts, draft, planet, Number(force), value) : draft;
}

/**
 * The force that a tap lands: the chosen one while units of it are left, or else the first that
 * has units left. With `planet`, only a force that can land there.
 */
export function chosenForce(
  facts: InvasionFacts,
  draft: LandingDraft,
  chosen: number | null,
  planet?: string,
): number | null {
  const lands = planet ? facts.planets.find((item) => item.planet === planet)?.lands : undefined;
  const can = (force: number) =>
    forceLeft(facts, draft, force) > 0 && (planet === undefined || !!lands?.includes(force));
  if (chosen !== null && can(chosen)) {
    return chosen;
  }
  const first = facts.forces.findIndex((_, force) => can(force));
  return first < 0 ? null : first;
}

/** The steps of the staged landings, in the order of the facts, and the end of the landings. */
export function stepsOf(facts: InvasionFacts, draft: LandingDraft): LandingStep[] {
  const steps: LandingStep[] = [];
  for (const { planet } of facts.planets) {
    for (const [force, count] of Object.entries(draft[planet] ?? {})) {
      const from = facts.forces[Number(force)];
      for (let unit = 0; from && unit < count; unit++) {
        steps.push({
          kind: "land",
          planet,
          unit: from.unit,
          damaged: from.damaged,
          from_system: from.from_system ?? null,
          from_planet: from.from_planet ?? null,
        });
      }
    }
  }
  steps.push({ kind: "done_committing" });
  return steps;
}

/**
 * The landings of some steps, staged again on the facts of now: a plan that stopped, an undo, a
 * reload. A step that no longer fits is left out.
 */
export function draftOf(facts: InvasionFacts, steps: PlanStep[]): LandingDraft {
  let draft: LandingDraft = {};
  for (const step of steps) {
    if (step.kind !== "land") {
      continue;
    }
    const force = facts.forces.findIndex(
      (item) =>
        item.unit === step.unit &&
        item.damaged === step.damaged &&
        (item.from_system ?? null) === step.from_system &&
        (item.from_planet ?? null) === step.from_planet,
    );
    if (force >= 0) {
      draft = setLanding(facts, draft, step.planet, force, (draft[step.planet]?.[force] ?? 0) + 1);
    }
  }
  return draft;
}
