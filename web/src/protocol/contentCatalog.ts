import { GENERATED_CONTENT_CATALOG } from "./generatedContentManifest.ts";

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
  description: string;
}

export const STRATEGY_CARDS = GENERATED_CONTENT_CATALOG.strategyCards;
export const SECRET_OBJECTIVES = GENERATED_CONTENT_CATALOG.secretObjectives;
export const PUBLIC_OBJECTIVES = GENERATED_CONTENT_CATALOG.publicObjectives;
export const ACTION_CARDS = GENERATED_CONTENT_CATALOG.actionCards;
export const TECHNOLOGIES = GENERATED_CONTENT_CATALOG.technologies;
export const EXPLORATION_CARDS = GENERATED_CONTENT_CATALOG.explorationCards;
export const PLANETS = GENERATED_CONTENT_CATALOG.planets;
export const ATTACHMENTS = GENERATED_CONTENT_CATALOG.attachments;

export function humanizeId(id: string): string {
  return id.replace(/_/g, " ").replace(/\b\w/g, (char) => char.toUpperCase());
}

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
