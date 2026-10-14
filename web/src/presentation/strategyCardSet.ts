import type { StrategyCardSetId } from "../protocol/types.ts";

/** What a lobby offers. The server validates; this list only has to match it (card_set.rs). */
export interface StrategyCardSetOption {
  id: StrategyCardSetId;
  /** Short name for the selector and the lobby row. */
  name: string;
  /** One honest line about what changes. */
  description: string;
}

/** The set a new game uses unless the host picks another. Mirrors `card_set::DEFAULT`. */
export const DEFAULT_STRATEGY_CARD_SET: StrategyCardSetId = "te";

export const STRATEGY_CARD_SETS: readonly StrategyCardSetOption[] = [
  {
    id: "te",
    name: "Thunder's Edge",
    description:
      "Construction and Warfare as printed in Thunder's Edge: Warfare is a free tactical action with token redistribution.",
  },
  {
    id: "pok",
    name: "Prophecy of Kings",
    description: "Construction and Warfare as printed in Prophecy of Kings.",
  },
  {
    id: "base_game_codex1",
    name: "Base game + Codex I",
    description: "The original Construction, with the Codex I Diplomacy.",
  },
];

export function isStrategyCardSetId(value: unknown): value is StrategyCardSetId {
  return STRATEGY_CARD_SETS.some((set) => set.id === value);
}

/** The selector label: the default is marked as such. */
export function strategyCardSetLabel(set: StrategyCardSetOption): string {
  return set.id === DEFAULT_STRATEGY_CARD_SET ? `${set.name} (default)` : set.name;
}

/** The name shown in the lobby; a lobby from an older server has none and plays PoK. */
export function strategyCardSetName(id: StrategyCardSetId | undefined): string {
  return (STRATEGY_CARD_SETS.find((set) => set.id === (id ?? "pok")) ?? STRATEGY_CARD_SETS[1]).name;
}
