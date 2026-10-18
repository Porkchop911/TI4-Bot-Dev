import type { Phrase } from "@ti4/shared/wire/index.ts";
import { sayPhrase, type SubjectNames } from "@ti4/shared/wire/phrase.ts";
import type { ActiveEffectDto, EffectPreviewDto } from "../protocol/types.ts";
import {
  findActionCardMeta,
  findSecretObjectiveMeta,
  findStrategyCardMeta,
  humanizeId,
} from "../protocol/contentCatalog.ts";
import { agendaNameFromId } from "./agendaStrip.ts";

/**
 * The engine's words for its effect facts, in the names of this page: what playing a reaction
 * card would do now (`EffectPreviewDto.phrase`) and which lingering card effects are in force
 * (`ActiveEffectDto.title`, `summary`, `expiry`). The engine words each kind; this file only says
 * the subjects that a phrase names (a seat, a system, units, a card). A preview without words
 * (an older server) gives `null` and the surface shows nothing.
 */

export interface EffectLabels {
  /** "You" for the viewer, the player's name otherwise. */
  seat: (id: string) => string;
  viewerSeat?: string | null;
  /** "System 27 (Lodor)"; defaults to "System 27". */
  system?: (id: string) => string;
}

const UNIT_PLURALS: Record<string, string> = {
  infantry: "infantry",
  fighter: "fighters",
  destroyer: "destroyers",
  cruiser: "cruisers",
  carrier: "carriers",
  dreadnought: "dreadnoughts",
  warsun: "war suns",
  war_sun: "war suns",
  flagship: "flagships",
  mech: "mechs",
  pds: "PDS",
  spacedock: "space docks",
  space_dock: "space docks",
};

/** "dreadnought" */
function unitName(unitType: string): string {
  return humanizeId(unitType).toLowerCase();
}

/** "2 cruisers", "1 dreadnought" */
function unitCount(unitType: string, count: number): string {
  const base = unitType.toLowerCase();
  return `${count} ${count === 1 ? unitName(base) : (UNIT_PLURALS[base] ?? `${base}s`)}`;
}

function cardName(alias: string | undefined | null): string | null {
  if (!alias) return null;
  return findActionCardMeta(alias)?.name ?? humanizeId(alias);
}

/** How this page says each subject of a phrase of the engine. */
function subjectNames(labels: EffectLabels): SubjectNames {
  return {
    seat: ({ player }) => labels.seat(player),
    seat_possessive: ({ player }) => (player === labels.viewerSeat ? "your" : `${labels.seat(player)}'s`),
    system: ({ system }) => (labels.system ?? ((id: string) => `System ${id}`))(system),
    planet: ({ planet }) => humanizeId(planet),
    unit: ({ unit_type }) => unitName(unit_type),
    units: ({ unit_type, count }) => unitCount(unit_type, count),
    agenda: ({ card }) => agendaNameFromId(card),
    strategy_card: ({ card }) => findStrategyCardMeta(card)?.name ?? humanizeId(card),
    note: ({ card }) => humanizeId(card),
    action_card: ({ card }) => cardName(card) ?? card,
    secret_objective: ({ card }) => findSecretObjectiveMeta(card)?.name ?? humanizeId(card),
  };
}

/** A phrase of the engine in the names of this page; `null` when it has no words. */
export function phraseText(phrase: Phrase | undefined, labels: EffectLabels): string | null {
  if (!phrase?.text) return null;
  return sayPhrase(phrase, subjectNames(labels));
}

/** What playing the card would do right now, as the engine words it; `null` without words. */
export function describePreview(preview: EffectPreviewDto, labels: EffectLabels): string | null {
  return phraseText(preview.phrase, labels);
}

export interface ActiveEffectText {
  /** The card (or pool) that causes the effect: "Morale Boost". */
  title: string;
  /** One short line: "+1 to all combat rolls". */
  summary: string;
  /** "until the end of this combat round" */
  expiry: string;
}

/** The strip's words for one lingering effect; `null` when the engine sent none. */
export function describeActiveEffect(effect: ActiveEffectDto, labels: EffectLabels): ActiveEffectText | null {
  const title = phraseText(effect.title, labels);
  const summary = phraseText(effect.summary, labels);
  if (title === null || summary === null) return null;
  return { title, summary, expiry: effect.expiry ?? "" };
}
