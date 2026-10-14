/**
 * Bought objectives ("Spend 8 resources", "Spend 16 influence", ...) are scored by paying the cost
 * at a scoring window. The server's progress for them is spending CAPACITY right now (the greatest
 * amount <= the cost payable from ready planets plus trade goods), not an accumulating counter, so
 * the UI labels it as such. The progress view carries no "bought" flag or unit, so both are read
 * from the objective's printed description.
 */
export type BoughtUnit = "resources" | "influence" | "trade goods";

const SINGLE_UNIT = /^spend (?:an? )?(\d+) (resources|influence|trade goods)\.?$/i;
const MIXED = /^spend \d+ influence, \d+ resources, and \d+ trade goods\.?$/i;

/** The spend unit of a single-unit bought objective, or null for anything else. */
export function boughtObjectiveUnit(description: string | undefined): BoughtUnit | null {
  const match = description ? SINGLE_UNIT.exec(description.trim()) : null;
  return match ? (match[2].toLowerCase() as BoughtUnit) : null;
}

/** Single-unit spends plus the mixed "All Three" spends. Token spends are not bought progress here. */
export function isBoughtObjective(description: string | undefined): boolean {
  return boughtObjectiveUnit(description) !== null || MIXED.test(description?.trim() ?? "");
}

export const BOUGHT_PROGRESS_HINT =
  "Bought objectives are paid for when you score them; this is how much you could pay right now with your ready planets and trade goods.";

export function boughtProgressText(
  have: number,
  threshold: number,
  unit: BoughtUnit | null,
  scoringWindowOpen: boolean,
): string {
  const amount = unit ? ` ${unit}` : "";
  const base = `can pay ${have} / ${threshold}${amount} now`;
  return scoringWindowOpen ? base : `${base} · pay at scoring`;
}
