import { humanizeId } from "@ti4/shared/content/names.ts";
import { CONTENT, catalogContent } from "./catalogStore.ts";

export { humanizeId };

export interface StrategyCardMeta {
  id: string;
  name: string;
  initiative: number;
  primaryText: string;
  secondaryText: string;
}

export interface ObjectiveMeta {
  id: string;
  name: string;
  phase: string;
  points: number;
  description: string;
}

export interface CardMeta {
  id: string;
  name: string;
  phase?: string;
  /** The printed timing of a reaction card ("When another player plays an action card ..."). */
  window?: string;
  description: string;
}

export const STRATEGY_CARDS = CONTENT.strategyCards;
export const SECRET_OBJECTIVES = CONTENT.secretObjectives;
export const PUBLIC_OBJECTIVES = CONTENT.publicObjectives;
export const ACTION_CARDS = CONTENT.actionCards;
export const TECHNOLOGIES = CONTENT.technologies;
export const EXPLORATION_CARDS = CONTENT.explorationCards;
export const PLANETS = CONTENT.planets;
export const ATTACHMENTS = CONTENT.attachments;
export const RELICS = CONTENT.relics;

function exactLookup<T>(catalog: Record<string, T>, id: string): T | undefined {
  return catalog[id];
}

export function findStrategyCardMeta(id: string): StrategyCardMeta | undefined {
  return exactLookup(STRATEGY_CARDS, id);
}

export function getStrategyCardMeta(id: string): StrategyCardMeta {
  return (
    findStrategyCardMeta(id) ?? {
      id,
      name: humanizeId(id),
      initiative: 0,
      primaryText: "",
      secondaryText: "",
    }
  );
}

export function findSecretObjectiveMeta(id: string): ObjectiveMeta | undefined {
  return exactLookup(SECRET_OBJECTIVES, id);
}

export function getSecretObjectiveMeta(id: string): ObjectiveMeta {
  return (
    findSecretObjectiveMeta(id) ?? {
      id,
      name: humanizeId(id),
      phase: "Secret",
      points: 1,
      description: "Secret Objective",
    }
  );
}

export function findPublicObjectiveMeta(id: string): ObjectiveMeta | undefined {
  return exactLookup(PUBLIC_OBJECTIVES, id);
}

export function getPublicObjectiveMeta(id: string): ObjectiveMeta {
  return (
    findPublicObjectiveMeta(id) ?? {
      id,
      name: humanizeId(id),
      phase: "Public",
      points: 1,
      description: "Public Objective",
    }
  );
}

export function findActionCardMeta(id: string): CardMeta | undefined {
  return exactLookup(ACTION_CARDS, id);
}

/** The action cards by printed name, of the catalog `content`. */
let actionCardsByName: { content: string | null; cards: Map<string, CardMeta> } | undefined;

/** Looks an action card up by its printed name, for decisions that only carry the label. */
export function findActionCardByName(name: string): CardMeta | undefined {
  if (actionCardsByName?.content !== catalogContent()) {
    actionCardsByName = {
      content: catalogContent(),
      cards: new Map(
        Object.values(ACTION_CARDS as Record<string, CardMeta>).map((card) => [
          card.name.toLowerCase(),
          card,
        ]),
      ),
    };
  }
  return actionCardsByName.cards.get(name.trim().toLowerCase());
}

export function getActionCardMeta(id: string): CardMeta {
  return (
    findActionCardMeta(id) ?? {
      id,
      name: humanizeId(id),
      description: "Action Card",
    }
  );
}

export interface TechnologyMeta extends CardMeta {
  types?: readonly string[];
  requirements?: string;
  faction?: string;
  source?: string;
  baseUpgrade?: string;
}

export function findTechnologyMeta(id: string): TechnologyMeta | undefined {
  return exactLookup(TECHNOLOGIES, id) as TechnologyMeta | undefined;
}

export function getTechnologyMeta(id: string): TechnologyMeta {
  return (
    findTechnologyMeta(id) ?? {
      id,
      name: humanizeId(id),
      types: [],
      description: "Technology",
    }
  );
}

export interface ExplorationCardMeta {
  id: string;
  name: string;
  type: string;
  resolution: string;
  description: string;
  flavorText?: string;
}

export function findExplorationCardMeta(key: string): ExplorationCardMeta | undefined {
  if (!key) return undefined;
  const exact = (EXPLORATION_CARDS as Record<string, ExplorationCardMeta>)[key];
  if (exact) return exact;
  const norm = key.toLowerCase().replace(/[^a-z0-9]/g, "");
  if (!norm) return undefined;
  for (const meta of Object.values(EXPLORATION_CARDS as Record<string, ExplorationCardMeta>)) {
    if (meta.id.toLowerCase() === norm) return meta;
    const cardNorm = meta.name.toLowerCase().replace(/[^a-z0-9]/g, "");
    if (norm.includes(cardNorm) || cardNorm.includes(norm)) {
      return meta;
    }
  }
  return undefined;
}

export function getExplorationCardMeta(key: string): ExplorationCardMeta {
  return (
    findExplorationCardMeta(key) ?? {
      id: key,
      name: humanizeId(key),
      type: "Frontier",
      resolution: "Instant",
      description: "",
    }
  );
}

export interface PlanetStaticMeta {
  id: string;
  name: string;
  resources: number;
  influence: number;
  techSpecialties?: readonly string[];
  /** Legendary planets: the ability card's name and printed text. */
  legendaryAbilityName?: string;
  legendaryAbilityText?: string;
}

export interface AttachmentStaticMeta {
  id: string;
  name: string;
  resourcesModifier: number;
  influenceModifier: number;
}

export function findPlanetMeta(id: string): PlanetStaticMeta | undefined {
  return exactLookup(PLANETS, id);
}

export function findAttachmentMeta(id: string): AttachmentStaticMeta | undefined {
  return exactLookup(ATTACHMENTS, id);
}

export function findRelicMeta(id: string): { id: string; name: string; text?: string } | undefined {
  return exactLookup(RELICS, id);
}

export function getActionCardDescription(cardName: string): string {
  const meta = getActionCardMeta(cardName);
  return meta.description || `Action Card: ${meta.name}`;
}

/** A Thunder's Edge breakthrough: a faction card a player earns, with a synergy. */
export interface BreakthroughMeta {
  id: string;
  name: string;
  faction?: string;
  /** The two technology colours its synergy ability reads ("BIOTIC", "WARFARE"). */
  synergy: readonly string[];
  text?: string;
}

export const BREAKTHROUGHS = CONTENT.breakthroughs as unknown as Record<
  string,
  BreakthroughMeta
>;

export function findBreakthroughMeta(id: string | null | undefined): BreakthroughMeta | undefined {
  return id ? exactLookup(BREAKTHROUGHS, id) : undefined;
}

const squash = (text: string): string =>
  text.toLowerCase().replace(/^the\s+/, "").replace(/[^a-z0-9]/g, "");

/** The breakthrough printed for a faction, by id ("sol") or printed name; earned or not. */
export function findBreakthroughForFaction(faction: string | null | undefined): BreakthroughMeta | undefined {
  if (!faction) return undefined;
  const cards = Object.values(BREAKTHROUGHS);
  const lower = faction.toLowerCase();
  const byId = cards.find((card) => card.faction === lower);
  if (byId) return byId;
  const wanted = squash(faction);
  const record = Object.values(
    CONTENT.factions as unknown as Record<string, { id: string; name: string }>,
  ).find((f) => squash(f.id) === wanted || squash(f.name) === wanted);
  return record ? cards.find((card) => card.faction === record.id) : undefined;
}
