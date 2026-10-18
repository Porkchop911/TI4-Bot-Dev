import { boostsFor, type BoostKind, type PlanetBoost } from "@ti4/shared/planets/values.ts";

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
