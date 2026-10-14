import { findAttachmentMeta, findExplorationCardMeta, findPlanetMeta, humanizeId } from "../protocol/contentCatalog.ts";
import { explorationResultText, type ExplorationRecordDto } from "./explorationToasts.ts";

/** One finished exploration, ready to list under a system or planet. */
export interface ExplorationEntry {
  round: number | null;
  player: string;
  /** Printed card name ("Mining World"). */
  card: string;
  /** "Rigel I (hazardous)" / "frontier token". */
  where: string;
  /** What the player got ("gained 2 commodities", "attached Mining World"). */
  result: string;
  /** What the map badge shows for it. */
  kind: "attachment" | "fragment" | "gain" | "frontier" | "none";
  planet: string | null;
}

/** Reads the state's `frontier_tokens` (system ids still holding an unexplored token). */
export function readFrontierTokens(state: Record<string, unknown> | null | undefined): string[] {
  const raw = state?.frontier_tokens;
  return Array.isArray(raw) ? raw.filter((id): id is string => typeof id === "string") : [];
}

function kindOf(record: ExplorationRecordDto): ExplorationEntry["kind"] {
  const outcome = record.outcome ?? "";
  if (outcome.startsWith("attached:")) return "attachment";
  if (outcome === "fragment") return "fragment";
  if (record.system && !record.planet) return "frontier";
  return record.result || Object.keys(record.gains ?? {}).length || record.placed?.length ? "gain" : "none";
}

export function explorationEntry(record: ExplorationRecordDto): ExplorationEntry {
  const trait = (record.deck || "").toLowerCase();
  const planet = record.planet ?? null;
  return {
    round: typeof record.round === "number" ? record.round : null,
    player: record.player,
    card: findExplorationCardMeta(record.card)?.name ?? humanizeId(record.card),
    where: planet
      ? `${findPlanetMeta(planet)?.name ?? humanizeId(planet)} (${trait})`
      : record.system
        ? `frontier token in system ${record.system}`
        : "frontier token",
    result: explorationResultText(record),
    kind: kindOf(record),
    planet,
  };
}

/**
 * The finished explorations that took place in a system: of its planets, or of its frontier token.
 * Records without an outcome (a card still being resolved) are left out.
 */
export function explorationsInSystem(
  records: readonly ExplorationRecordDto[],
  systemId: string,
  planetIds: readonly string[],
): ExplorationEntry[] {
  const planets = new Set(planetIds);
  return records
    .filter((r) => r.outcome)
    .filter((r) => (r.planet ? planets.has(r.planet) : r.system === systemId))
    .map(explorationEntry);
}

/** The names of the attachments on a planet, with the values they add ("Mining World (+2 resources)"). */
export function attachmentLabels(ids: readonly string[]): string[] {
  return ids.map((id) => {
    const meta = findAttachmentMeta(id);
    if (!meta) return humanizeId(id);
    const gains = [
      meta.resourcesModifier ? `${meta.resourcesModifier > 0 ? "+" : ""}${meta.resourcesModifier} resources` : "",
      meta.influenceModifier ? `${meta.influenceModifier > 0 ? "+" : ""}${meta.influenceModifier} influence` : "",
    ].filter(Boolean);
    return gains.length ? `${meta.name} (${gains.join(", ")})` : meta.name;
  });
}
