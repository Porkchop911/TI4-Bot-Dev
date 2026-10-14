import { BoardView, ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import {
  findPlanetMeta,
  humanizeId,
} from "../protocol/contentCatalog.ts";
import { isDeclineOption, isPlanetSelectionChoice, optionPlanetId } from "./choiceModel.ts";
import { formatDecisionSource } from "./decisionSource.ts";

export interface PlanetDetails {
  id: string;
  name: string;
  systemId: string | null;
  resources: number | null;
  influence: number | null;
  traits: string[];
  techSpecialties: string[];
  legendary: boolean;
  controlledBy: string | null;
  exhausted: boolean;
}

/** Finds the system containing a planet, from the dynamic systems first, then the static tiles. */
export function findPlanetSystem(planetId: string, board?: BoardView | null): string | null {
  if (!board) return null;
  for (const [systemId, system] of Object.entries(board.systems ?? {})) {
    if (system.planets?.[planetId]) return systemId;
  }
  const tile = board.map_tiles?.find((t) => t.planets?.some((p) => p.id === planetId));
  return tile?.system_id ?? null;
}

/** The system an option points at: `payload.system` when present, else the planet's system. */
export function resolveOptionSystem(opt: ChoiceOptionDto, board?: BoardView | null): string | null {
  const system = opt.payload?.system;
  if (typeof system === "string" && system) return system;
  if (typeof system === "number") return String(system);
  const planet = optionPlanetId(opt);
  return planet ? findPlanetSystem(planet, board) : null;
}

export function getPlanetDetails(planetId: string, board?: BoardView | null): PlanetDetails {
  const systemId = findPlanetSystem(planetId, board);
  const tile = board?.map_tiles?.find((t) => t.planets?.some((p) => p.id === planetId));
  const meta = tile?.planets?.find((p) => p.id === planetId);
  const catalog = findPlanetMeta(planetId);
  const dynamic = systemId ? board?.systems?.[systemId]?.planets?.[planetId] : undefined;
  return {
    id: planetId,
    name: meta?.label || catalog?.name || humanizeId(planetId),
    systemId,
    resources: meta?.resources ?? catalog?.resources ?? null,
    influence: meta?.influence ?? catalog?.influence ?? null,
    traits: meta?.traits ?? [],
    techSpecialties: [...(meta?.tech_specialties ?? catalog?.techSpecialties ?? [])],
    legendary: Boolean(meta?.legendary),
    controlledBy: dynamic?.controlled_by ?? null,
    exhausted: Boolean(dynamic?.exhausted),
  };
}

/** "3R/1I" style resource/influence summary, or null when unknown. */
export function formatPlanetStats(details: PlanetDetails): string | null {
  if (details.resources === null || details.influence === null) return null;
  return `${details.resources}R/${details.influence}I`;
}

export { formatDecisionSource } from "./decisionSource.ts";

export interface DecisionDescription {
  /** Card, ability, relic, technology … that is asking, e.g. "Mining Initiative". */
  sourceLabel: string | null;
  /** What is being asked, e.g. "mine which planet". */
  actionPrompt: string;
  /** The verb applied to the picked planet when the option label is just the planet name. */
  actionVerb: string;
}

const GENERIC_SOURCES = new Set(["Legendary", "Place Structure"]);

/** Readable names for decisions whose structured source is only a rule number. */
const SUBTYPE_SOURCE_LABELS: Record<string, string> = {
  cast_vote: "Agenda Vote",
};

/**
 * Splits an engine prompt such as "Mining Initiative: mine which planet" into the source and the
 * action, falling back to the structured `context.source`.
 */
export function describePlanetDecision(choice: PendingChoiceDto): DecisionDescription {
  const prompt = (choice.prompt ?? "").trim();
  const formatted = formatDecisionSource(choice.context?.source);
  const subtypeLabel = SUBTYPE_SOURCE_LABELS[choice.context?.subtype ?? ""];
  const contextSource =
    subtypeLabel && (!formatted || /^Rule /.test(formatted)) ? subtypeLabel : formatted;
  const match = /^([^:]{2,60}):\s*(.+)$/.exec(prompt);
  let sourceLabel = contextSource;
  let actionPrompt = prompt;
  if (match) {
    const head = match[1].trim();
    // An authored prompt head ("The Acropolis") is more specific than a generic content alias.
    if (!contextSource || GENERIC_SOURCES.has(contextSource) || /^Rule /.test(contextSource))
      sourceLabel = head;
    actionPrompt = head.toLowerCase() === (sourceLabel ?? "").toLowerCase() ? match[2] : prompt;
  }
  if (sourceLabel && actionPrompt.toLowerCase() === sourceLabel.toLowerCase())
    actionPrompt = "choose a planet";
  if (!actionPrompt) actionPrompt = "choose a planet";
  const verb = actionPrompt
    .replace(/\b(which|what)\b.*$/i, "")
    .replace(/\b(choose|select|pick)\b.*$/i, "")
    .replace(/\b(a|an|one)\s+.*$/i, "")
    .trim();
  return { sourceLabel, actionPrompt, actionVerb: verb || "target" };
}

/** The option label without a repeated "Source: " prefix. */
export function optionActionLabel(opt: ChoiceOptionDto, sourceLabel: string | null): string {
  const label = (opt.label ?? "").trim();
  if (sourceLabel) {
    const prefix = `${sourceLabel.toLowerCase()}:`;
    if (label.toLowerCase().startsWith(prefix)) return label.slice(prefix.length).trim();
  }
  return label;
}

/** True when the option label says more than the planet's name (e.g. "place pds on lodor"). */
export function isSpecificOptionLabel(
  opt: ChoiceOptionDto,
  sourceLabel: string | null,
  planet: PlanetDetails,
): boolean {
  const label = optionActionLabel(opt, sourceLabel).toLowerCase();
  if (!label) return false;
  return label !== planet.id.toLowerCase() && label !== planet.name.toLowerCase();
}

export interface PlanetCandidate {
  planetId: string;
  systemId: string | null;
  options: ChoiceOptionDto[];
}

/** Candidate planets in offer order, each with every option that targets it. */
export function groupPlanetCandidates(
  choice: PendingChoiceDto,
  board?: BoardView | null,
): PlanetCandidate[] {
  const byPlanet = new Map<string, PlanetCandidate>();
  for (const opt of choice.options) {
    const planetId = optionPlanetId(opt);
    if (!planetId) continue;
    const existing = byPlanet.get(planetId);
    if (existing) existing.options.push(opt);
    else
      byPlanet.set(planetId, {
        planetId,
        systemId: resolveOptionSystem(opt, board),
        options: [opt],
      });
  }
  return [...byPlanet.values()];
}

export type MapTargetSelection =
  | { kind: "select"; optionId: string | undefined; planetId: string | null }
  | { kind: "ignore" };

/**
 * Resolves a map click into a selection for the pending choice.
 *
 * - Planet click: options whose `payload.planet` matches. One match selects it; several (one per
 *   structure) select only the planet so the bar can offer the choice. Payment/vote basket ids
 *   (`exhaust|planet`, bare planet id) remain a fallback.
 * - Hex click during a planet selection: picks the planet only when the system has exactly one
 *   candidate planet; otherwise the click is just an inspection.
 * - Hex click otherwise: the option targeting that system.
 */
export function resolveMapTargetSelection(
  choice: PendingChoiceDto | null,
  systemId: string,
  planetId?: string,
  board?: BoardView | null,
): MapTargetSelection {
  if (!choice) return { kind: "ignore" };
  if (planetId) {
    const matches = choice.options.filter((o) => optionPlanetId(o) === planetId);
    if (matches.length === 1) return { kind: "select", optionId: matches[0].id, planetId };
    if (matches.length > 1) return { kind: "select", optionId: undefined, planetId };
    const fallback = choice.options.find(
      (o) =>
        o.id === `exhaust|${planetId}` ||
        o.id === planetId ||
        o.id.startsWith(`exhaust|${planetId}|`),
    );
    return { kind: "select", optionId: fallback?.id, planetId: fallback ? planetId : null };
  }
  if (isPlanetSelectionChoice(choice)) {
    const inSystem = groupPlanetCandidates(choice, board).filter((c) => c.systemId === systemId);
    if (inSystem.length === 1)
      return resolveMapTargetSelection(choice, systemId, inSystem[0].planetId, board);
    return { kind: "ignore" };
  }
  const match = choice.options.find(
    (o) => !isDeclineOption(o) && String(o.payload?.system ?? o.payload?.to ?? o.id) === systemId,
  );
  return { kind: "select", optionId: match?.id, planetId: null };
}

export interface StructureStep {
  step: number;
  of: number;
  onlyPds: boolean;
}

/**
 * Which placement of a card's structure sequence a `place_structure` decision is (Construction:
 * "1 of 2", then "2 of 2, PDS only"). `null` for any other decision or when the engine sent no
 * step.
 */
export function describeStructureStep(
  choice: Pick<PendingChoiceDto, "context" | "details">,
): StructureStep | null {
  if (choice.context?.subtype !== "place_structure") return null;
  const { step, of, only_pds: onlyPds } = choice.details ?? {};
  if (typeof step !== "number" || typeof of !== "number" || of < 1) return null;
  return { step, of, onlyPds: onlyPds === true };
}

export function formatStructureStep(view: StructureStep): string {
  return `Structure ${view.step} of ${view.of}${view.onlyPds ? " · PDS only" : ""}`;
}
