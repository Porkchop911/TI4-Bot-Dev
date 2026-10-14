import type { BoardView, ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { findPlanetMeta, humanizeId } from "../protocol/contentCatalog.ts";
import { findPlanetSystem } from "./planetSelection.ts";

const WINDOWS: Record<string, string> = {
  legendary_end_of_turn: "at the end of your turn",
  legendary_pass: "as you pass",
};

export interface LegendaryAbilityView {
  /** The ability card, e.g. "The Atrament". */
  ability: string;
  planet: string;
  /** "2R/1I", or null when the planet is not in the catalog. */
  stats: string | null;
  /** The system the planet sits in, when the board shows it. */
  systemId: string | null;
  /** The printed text of the ability card. */
  text: string | null;
}

/** When the legendary ability window is open ("at the end of your turn"); `null` for other decisions. */
export function legendaryWindow(choice: Pick<PendingChoiceDto, "context">): string | null {
  return WINDOWS[choice.context?.subtype ?? ""] ?? null;
}

/** One offered legendary ability: its card text and the planet that carries it. */
export function describeLegendaryOption(
  choice: Pick<PendingChoiceDto, "context">,
  option: Pick<ChoiceOptionDto, "id" | "kind" | "label">,
  board?: BoardView | null,
): LegendaryAbilityView | null {
  if (legendaryWindow(choice) === null || option.kind !== "legendary") return null;
  const meta = findPlanetMeta(option.id);
  return {
    ability: meta?.legendaryAbilityName ?? option.label,
    planet: meta?.name ?? humanizeId(option.id),
    stats: meta ? `${meta.resources}R/${meta.influence}I` : null,
    systemId: findPlanetSystem(option.id, board),
    text: meta?.legendaryAbilityText?.trim() || null,
  };
}
