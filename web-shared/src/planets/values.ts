// A planet's resources, influence and technology specialties as they stand now: the printed values
// (the map tile metadata) with what attachments, laws and docks change. The changes come from the
// engine's own planet value function (`PlanetView`), so what an app shows cannot differ from what
// payment counts.
import type { PlanetValueBoost, PlanetView } from "../wire/index.ts";

/** One source that raises a planet, e.g. `{ source: "Mining World", resources: 2, influence: 0 }`. */
export type PlanetBoost = PlanetValueBoost;

export type BoostKind = "resources" | "influence";

/** The printed values of a planet. */
export interface PrintedValues {
  resources: number;
  influence: number;
}

/** The sources that add to one kind, as `{ source, amount }`. */
export function boostsFor(
  boosts: readonly PlanetBoost[] | undefined,
  kind: BoostKind,
): { source: string; amount: number }[] {
  return (boosts ?? [])
    .map((b) => ({ source: b.source, amount: b[kind] }))
    .filter((b) => b.amount !== 0);
}

/** Printed values plus boosts. */
export function applyBoosts(
  printedResources: number,
  printedInfluence: number,
  boosts: readonly PlanetBoost[] | undefined,
): PrintedValues {
  let resources = printedResources;
  let influence = printedInfluence;
  for (const b of boosts ?? []) {
    resources += b.resources;
    influence += b.influence;
  }
  return { resources, influence };
}

/** Boosts as sent on the wire (absent when nothing raises the planet). */
export function wireBoosts(
  view: Pick<PlanetView, "value_boosts"> | null | undefined,
): PlanetBoost[] {
  return (view?.value_boosts ?? []).map((b) => ({
    source: b.source,
    resources: b.resources,
    influence: b.influence,
  }));
}

/**
 * The resources and influence of a planet now: the engine's totals (`PlanetView.resources`,
 * `.influence`) when it sends them, else the printed values plus `value_boosts`.
 */
export function planetValuesNow(
  printed: PrintedValues,
  view: Pick<PlanetView, "resources" | "influence" | "value_boosts"> | null | undefined,
): PrintedValues {
  const raised = applyBoosts(printed.resources, printed.influence, view?.value_boosts);
  return {
    resources: view?.resources ?? raised.resources,
    influence: view?.influence ?? raised.influence,
  };
}

/** The technology specialties of a planet now: the engine's (`PlanetView.tech_specialties`), else the printed ones. */
export function techSpecialtiesNow(
  printed: readonly string[] | null | undefined,
  view: Pick<PlanetView, "tech_specialties"> | null | undefined,
): string[] {
  return [...(view?.tech_specialties ?? printed ?? [])];
}
