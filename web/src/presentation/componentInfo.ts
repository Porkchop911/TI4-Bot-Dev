import type { ChoiceOption } from "../protocol/types.ts";
import { CONTENT } from "../protocol/catalogStore.ts";
import {
  findExplorationCardMeta,
  findTechnologyMeta,
  humanizeId,
  TECHNOLOGIES,
  type TechnologyMeta,
} from "../protocol/contentCatalog.ts";
import { breakthroughNote } from "./breakthroughs.ts";
import {
  findFactionRecord,
  type FactionAbilityRecord,
  type LeaderRecord,
  type PromissoryNoteRecord,
} from "./factionInfo.ts";

/**
 * What a component action (the turn menu's Components entries) does, in printed words: the card's
 * name, what kind of component it is, whose it is, when it is used, what it costs and its rules
 * text. Everything is looked up from the content catalog by the option id; the engine's label is
 * the fallback, so an unknown component still shows what the engine said. Display only: the
 * option id that is submitted never changes.
 */
export interface ComponentDescription {
  name: string;
  /** "Technology", "Agent", "Relic", ... */
  kind: string;
  /** The printed faction name of the card's owner, when the card belongs to a faction. */
  owner: string | null;
  /** When it is used, as printed ("ACTION", "At the start of a ground combat round:"). */
  timing: string | null;
  /** What using it spends ("Exhausts the card", "Cost: 1 strategy token"). */
  cost: string | null;
  /** The rules text; `null` only when neither the catalog nor the engine has any. */
  text: string | null;
  /** Anything worth adding (borrowed via Ssruu, which technology Enigmatic Device researches). */
  note: string | null;
}

interface RelicRecord {
  id: string;
  name: string;
  text?: string;
}

const LEADERS = CONTENT.leaders as unknown as Record<string, LeaderRecord>;
const ABILITIES = CONTENT.factionAbilities as unknown as Record<
  string,
  FactionAbilityRecord
>;
const NOTES = CONTENT.promissoryNotes as unknown as Record<
  string,
  PromissoryNoteRecord
>;
const RELICS = CONTENT.relics as unknown as Record<string, RelicRecord>;

const norm = (text: string) => text.toLowerCase().replace(/[^a-z0-9]/g, "");

export function findRelicRecord(id: string): RelicRecord | undefined {
  return RELICS[id] ?? Object.values(RELICS).find((r) => norm(r.name) === norm(id));
}

export function findLeaderRecord(id: string): LeaderRecord | undefined {
  return LEADERS[id];
}

const ownerName = (faction: string | undefined): string | null =>
  faction ? (findFactionRecord(faction)?.name ?? humanizeId(faction)) : null;

/** Cost words read from a label or rules text. */
export function componentCost(text: string): string | null {
  const lower = text.toLowerCase();
  if (/strategy (token|pool)/.test(lower)) return "Cost: 1 strategy token";
  const resources = /(\d+) resources/.exec(lower);
  if (resources && /spend/.test(lower)) return `Cost: ${resources[1]} resources`;
  const goods = /spend (\d+) trade goods/.exec(lower);
  if (goods) return `Cost: ${goods[1]} trade goods`;
  if (/\bpurge (this|the) card\b|\bpurge this\b/.test(lower)) return "Purged after use";
  if (/\bexhaust (this|the) (card|agent)\b/.test(lower)) return "Exhausts the card";
  return null;
}

/** The "ACTION:" window a rules text opens with, or `null`. */
function actionTiming(text: string | undefined): string | null {
  return text && /\bACTION\b/.test(text) ? "ACTION" : null;
}

const techByName = (name: string): TechnologyMeta | undefined =>
  Object.values(TECHNOLOGIES as unknown as Record<string, TechnologyMeta>).find(
    (t) => norm(t.name) === norm(name),
  );

const noteByName = (name: string): PromissoryNoteRecord | undefined =>
  Object.values(NOTES).find((n) => norm(n.name) === norm(name));

function fromTech(meta: TechnologyMeta, label: string): ComponentDescription {
  const text = meta.description ?? null;
  return {
    name: meta.name,
    kind: meta.faction ? "Faction technology" : "Technology",
    owner: ownerName(meta.faction),
    timing: actionTiming(text ?? undefined),
    cost: componentCost(text ?? label) ?? componentCost(label),
    text: text || label,
    note: null,
  };
}

function fromNote(note: PromissoryNoteRecord, label: string): ComponentDescription {
  return {
    name: note.name,
    kind: "Promissory note",
    owner: ownerName(note.faction),
    timing: actionTiming(note.text),
    cost: null,
    text: note.text || label,
    note: null,
  };
}

function fromAbility(ability: FactionAbilityRecord, label: string): ComponentDescription {
  const effect = ability.windowEffect ?? ability.permanentEffect ?? null;
  return {
    name: ability.name,
    kind: "Faction ability",
    owner: ownerName(ability.faction),
    timing: ability.window ?? null,
    cost: componentCost(effect ?? label) ?? componentCost(label),
    text: effect || label,
    note: null,
  };
}

function fromLeader(leader: LeaderRecord, borrowed: boolean): ComponentDescription {
  const kind = humanizeId(leader.type);
  const window = leader.abilityWindow ? leader.abilityWindow.replace(/:\s*$/, "") : null;
  const text = leader.abilityText ?? null;
  return {
    name: leader.name,
    kind,
    owner: ownerName(leader.faction),
    timing: window,
    cost:
      leader.type === "hero"
        ? "Purged after use"
        : leader.type === "agent"
          ? "Exhausts the agent"
          : null,
    text: text ?? null,
    note: borrowed ? "Used through Ssruu (Yssaril agent), which is exhausted" : null,
  };
}

const fallback = (option: ChoiceOption, kind: string): ComponentDescription => ({
  name: option.label,
  kind,
  owner: null,
  timing: null,
  cost: componentCost(option.label),
  text: null,
  note: null,
});

/** Map an engine component option to its printed description. */
export function describeComponent(
  option: ChoiceOption,
  viewerFaction?: string | null,
): ComponentDescription {
  const id = option.id;
  const label = option.label;

  if (id.startsWith("component|tech|")) {
    const meta = findTechnologyMeta(id.slice("component|tech|".length));
    return meta ? fromTech(meta, label) : fallback(option, "Technology");
  }

  if (id.startsWith("component|leader|")) {
    const parts = id.slice("component|leader|".length).split("|");
    // `component|leader|yssarilagent|<agent>`: another seat's agent borrowed through Ssruu.
    const borrowed = parts.length > 1;
    const leader = findLeaderRecord(parts[parts.length - 1]);
    if (leader) return fromLeader(leader, borrowed);
    return fallback(option, "Leader");
  }

  if (id.startsWith("relic|")) {
    const relic = findRelicRecord(id.slice("relic|".length));
    if (!relic) return fallback(option, "Relic");
    return {
      name: relic.name,
      kind: "Relic",
      owner: null,
      timing: actionTiming(relic.text),
      cost: componentCost(relic.text ?? ""),
      text: relic.text || label,
      note: null,
    };
  }

  if (id.startsWith("purge|")) {
    const trait = id.slice("purge|".length).toLowerCase();
    return {
      name: `${humanizeId(trait)} relic fragments`,
      kind: "Relic fragments",
      owner: null,
      timing: "ACTION",
      cost: `Purge 3 ${trait} fragments`,
      text: `ACTION: Purge 3 ${trait} relic fragments (unknown fragments count as any type) to gain 1 relic.`,
      note: null,
    };
  }

  if (id.startsWith("play_area:")) {
    const [card, tech] = id.slice("play_area:".length).split(":");
    const meta = findExplorationCardMeta(card);
    const techName = tech ? (findTechnologyMeta(tech)?.name ?? humanizeId(tech)) : null;
    return {
      name: meta?.name ?? label,
      kind: "Exploration card",
      owner: null,
      timing: actionTiming(meta?.description),
      cost: componentCost(label) ?? componentCost(meta?.description ?? ""),
      text: meta?.description || label,
      note: techName ? `Researches ${techName}` : null,
    };
  }

  if (id.startsWith("component|expedition|")) {
    const what = label.replace(/^Thunder's Edge expedition:\s*/i, "");
    return {
      name: `Expedition: ${what}`,
      kind: "Thunder's Edge expedition",
      owner: null,
      timing: "ACTION",
      cost: componentCost(label),
      text: `Pay for and resolve this expedition slice: ${what}.`,
      // The slice that grants the breakthrough names the card so the choice is an informed one.
      note: breakthroughNote(viewerFaction),
    };
  }

  if (id.startsWith("faction|")) {
    const key = id.slice("faction|".length);
    const ability = ABILITIES[key] ?? ABILITIES[key.split("|").pop() ?? ""];
    if (ability) return fromAbility(ability, label);
    // Production Biomes (Hacan technology) and Trade Convoys (Hacan note) arrive as faction ids.
    const printed = label.split(":")[0].trim();
    const tech = techByName(printed) ?? techByName(humanizeId(key));
    if (tech) return fromTech(tech, label);
    const note = noteByName(printed) ?? noteByName(humanizeId(key));
    if (note) return fromNote(note, label);
    return fallback(option, "Faction ability");
  }

  // Anything else: try every id segment against the catalog before giving up.
  for (const segment of id.split(/[|:]/).reverse()) {
    const leader = findLeaderRecord(segment);
    if (leader) return fromLeader(leader, false);
    const tech = findTechnologyMeta(segment);
    if (tech) return fromTech(tech, label);
    const ability = ABILITIES[segment];
    if (ability) return fromAbility(ability, label);
    const note = NOTES[segment];
    if (note) return fromNote(note, label);
  }
  return fallback(option, "Component action");
}
