import type { PlanetValueBoost } from "../protocol/types.ts";

/**
 * Planet resources and influence as they stand now: the printed value (map tile metadata) plus what
 * attachments, laws and docks add. The extra comes from the engine's own planet value function
 * (`PlanetView.value_boosts`), so what the UI shows cannot differ from what payment counts.
 */
export interface PlanetBoost {
  /** Card name, e.g. "Mining World". */
  source: string;
  /** Extra resources / influence this source adds (never 0 for both). */
  resources: number;
  influence: number;
}

export type BoostKind = "resources" | "influence";

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
): { resources: number; influence: number } {
  let resources = printedResources;
  let influence = printedInfluence;
  for (const b of boosts ?? []) {
    resources += b.resources;
    influence += b.influence;
  }
  return { resources, influence };
}

/** Boosts as sent on the wire (absent when nothing raises the planet). */
export function wireBoosts(view: { value_boosts?: PlanetValueBoost[] } | null | undefined): PlanetBoost[] {
  return (view?.value_boosts ?? []).map((b) => ({ source: b.source, resources: b.resources, influence: b.influence }));
}

/** "Mining World: +2 resources" lines for a tooltip; empty when nothing boosts this kind. */
export function boostLines(boosts: readonly PlanetBoost[] | undefined, kind: BoostKind): string[] {
  const word = kind === "resources" ? "resources" : "influence";
  return boostsFor(boosts, kind).map((b) => `${b.source}: ${b.amount > 0 ? "+" : ""}${b.amount} ${word}`);
}

/** A short label of everything boosting a planet, e.g. "Mining World +2R, Dyson Sphere +2R +1I". */
export function boostSummary(boosts: readonly PlanetBoost[] | undefined): string {
  return (boosts ?? [])
    .map((b) => {
      const parts = [b.resources ? `${b.resources > 0 ? "+" : ""}${b.resources}R` : "", b.influence ? `${b.influence > 0 ? "+" : ""}${b.influence}I` : ""];
      return `${b.source} ${parts.filter(Boolean).join(" ")}`;
    })
    .join(", ");
}
