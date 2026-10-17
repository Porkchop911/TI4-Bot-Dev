import { CONTENT, catalogContent } from "../protocol/catalogStore.ts";
import { findBreakthroughForFaction, findTechnologyMeta } from "../protocol/contentCatalog.ts";
import { synergyText } from "./breakthroughs.ts";

/**
 * Faction, unit and leader text for the info cards. Every string here is read out of the content
 * catalog (the corpus the engine plays); nothing is written by hand. A missing record yields a
 * missing field, never an invented one.
 */

export interface UnitRecord {
  id: string;
  baseType: string;
  name: string;
  subtitle?: string;
  faction?: string;
  source?: string;
  cost?: number;
  moveValue?: number;
  capacityValue?: number;
  combatHitsOn?: number;
  combatDieCount?: number;
  sustainDamage?: boolean;
  afbHitsOn?: number;
  afbDieCount?: number;
  bombardHitsOn?: number;
  bombardDieCount?: number;
  spaceCannonHitsOn?: number;
  spaceCannonDieCount?: number;
  deepSpaceCannon?: boolean;
  planetaryShield?: boolean;
  disablesPlanetaryShield?: boolean;
  /** As printed: "5", or what is added to the resources of the planet ("+2"). */
  productionValue?: string;
  requiredTechId?: string;
  upgradesFromUnitId?: string;
  upgradesToUnitId?: string;
  ability?: string;
}

export interface LeaderRecord {
  id: string;
  faction: string;
  type: string;
  name: string;
  title?: string;
  abilityName?: string;
  abilityWindow?: string;
  abilityText?: string;
  unlockCondition?: string;
}

export interface FactionAbilityRecord {
  id: string;
  name: string;
  faction?: string;
  permanentEffect?: string;
  window?: string;
  windowEffect?: string;
}

export interface PromissoryNoteRecord {
  id: string;
  name: string;
  faction?: string;
  text?: string;
}

export interface FactionRecord {
  id: string;
  name: string;
  commodities?: number;
  startingTech: readonly string[];
  factionTech: readonly string[];
  abilities: readonly string[];
  leaders: readonly string[];
  promissoryNotes: readonly string[];
  units: readonly string[];
  source?: string;
}

const FACTIONS = CONTENT.factions as unknown as Record<string, FactionRecord>;
const UNITS = CONTENT.units as unknown as Record<string, UnitRecord>;
const LEADERS = CONTENT.leaders as unknown as Record<string, LeaderRecord>;
const FACTION_ABILITIES = CONTENT.factionAbilities as unknown as Record<
  string,
  FactionAbilityRecord
>;
const PROMISSORY_NOTES = CONTENT.promissoryNotes as unknown as Record<
  string,
  PromissoryNoteRecord
>;

const normalize = (text: string): string =>
  text
    .toLowerCase()
    .replace(/^the\s+/, "")
    .replace(/[^a-z0-9]/g, "");

/** The factions by normalized id and printed name, of the catalog `content`. */
let factionsByName: { content: string | null; factions: Map<string, FactionRecord> } | undefined;

/** A faction by its id ("hacan") or its printed name ("The Barony of Letnev"); `undefined` when the catalog has none. */
export function findFactionRecord(key: string | null | undefined): FactionRecord | undefined {
  if (!key) return undefined;
  const exact = FACTIONS[key] ?? FACTIONS[key.toLowerCase()];
  if (exact) return exact;
  if (factionsByName?.content !== catalogContent()) {
    const factions = new Map<string, FactionRecord>();
    for (const faction of Object.values(FACTIONS)) {
      factions.set(normalize(faction.id), faction);
      factions.set(normalize(faction.name), faction);
    }
    factionsByName = { content: catalogContent(), factions };
  }
  return factionsByName.factions.get(normalize(key));
}

export function findUnitRecord(id: string): UnitRecord | undefined {
  return UNITS[id];
}

/** The ten unit types a player can build or own; the key a build option carries for generic units. */
const BASE_TYPES = [
  "fighter",
  "destroyer",
  "cruiser",
  "carrier",
  "dreadnought",
  "flagship",
  "warsun",
  "infantry",
  "mech",
  "pds",
  "spacedock",
] as const;

/** Which printed card a unit key refers to once the seat's faction and upgrades are known. */
export interface ResolvedUnit {
  unit: UnitRecord;
  /** The unit card is the faction's own version of this unit (flagship, mech, faction variant). */
  factionSpecific: boolean;
  /** The seat owns the upgrade that makes this the "II" card. */
  upgraded: boolean;
  /** The unit this card replaces when the seat has not researched the upgrade. */
  baseTier?: UnitRecord;
}

function tierOf(
  units: readonly UnitRecord[],
  ownedTechs: ReadonlySet<string>,
): { unit: UnitRecord; upgraded: boolean; base: UnitRecord } | undefined {
  let base = units.find((u) => !u.upgradesFromUnitId) ?? units[0];
  if (!base) return undefined;
  let current = base;
  let upgraded = false;
  for (let guard = 0; guard < 4 && current.upgradesToUnitId; guard++) {
    const next = UNITS[current.upgradesToUnitId];
    if (!next?.requiredTechId || !ownedTechs.has(next.requiredTechId)) break;
    current = next;
    upgraded = true;
  }
  return { unit: current, upgraded, base };
}

/**
 * The unit card for a unit key ("carrier", "mech", "sol_carrier2") as it applies to a seat.
 * With a faction, its own flagship/mech/variant replaces the generic card; with owned technology ids,
 * the upgraded card replaces the base one. Without either the generic base card answers.
 */
export function resolveUnit(
  unitKey: string,
  factionKey?: string | null,
  ownedTechs: readonly string[] = [],
): ResolvedUnit | undefined {
  const owned = new Set(ownedTechs);
  const exact = UNITS[unitKey];
  const baseType: string | undefined =
    exact?.baseType ?? (BASE_TYPES as readonly string[]).find((t) => t === unitKey.toLowerCase());
  if (!baseType) return undefined;

  const faction = findFactionRecord(factionKey);
  const ofType = (ids: readonly string[]) =>
    ids.map((id) => UNITS[id]).filter((u): u is UnitRecord => !!u && u.baseType === baseType);

  // A key that names a specific card ("sol_carrier2", "carrier2") with no seat to apply it to.
  if (exact && !faction && (exact.faction || exact.upgradesFromUnitId)) {
    return { unit: exact, factionSpecific: !!exact.faction, upgraded: !!exact.upgradesFromUnitId };
  }

  const factionUnits = faction ? ofType(faction.units) : [];
  // The faction's own card family: only when it is faction-specific, not the generic one it lists.
  const own = factionUnits.filter((u) => u.faction === faction?.id);
  const family = own.length > 0 ? own : factionUnits.filter((u) => !u.faction);
  if (family.length > 0) {
    const tier = tierOf(family, owned);
    if (tier) {
      return {
        unit: tier.unit,
        factionSpecific: own.length > 0,
        upgraded: tier.upgraded,
        baseTier: tier.upgraded ? tier.base : undefined,
      };
    }
  }
  // No faction (or a faction that lacks this unit, e.g. a mech outside Prophecy of Kings): the
  // card the key names, else the generic base card of that type.
  const generic = ofType(Object.keys(UNITS)).filter(
    (u) => !u.faction && u.source === "base" && u.id !== "nowarsun",
  );
  const tier = tierOf(generic, owned);
  if (!tier) {
    return exact ? { unit: exact, factionSpecific: !!exact.faction, upgraded: false } : undefined;
  }
  return { unit: tier.unit, factionSpecific: false, upgraded: tier.upgraded };
}

export interface UnitFact {
  label: string;
  value: string;
}

export interface UnitDescription {
  id: string;
  name: string;
  subtitle?: string;
  typeLabel: string;
  facts: UnitFact[];
  /** The abilities the numbers carry (bombardment, sustain damage, ...), as short lines. */
  keywords: string[];
  /** The card's printed ability text, verbatim. */
  text: string[];
  factionSpecific: boolean;
  upgraded: boolean;
}

const TYPE_LABELS: Record<string, string> = {
  fighter: "Fighter",
  destroyer: "Destroyer",
  cruiser: "Cruiser",
  carrier: "Carrier",
  dreadnought: "Dreadnought",
  flagship: "Flagship",
  warsun: "War Sun",
  infantry: "Infantry",
  mech: "Mech",
  pds: "PDS",
  spacedock: "Space Dock",
};

const dice = (hitsOn: number, count: number | undefined): string =>
  `${hitsOn}${count && count > 1 ? ` (x${count})` : ""}`;

function formatCost(cost: number): string {
  // Fighters and infantry are bought two to a resource.
  return cost > 0 && cost < 1 ? `${1 / cost} for 1` : String(cost);
}

/** The card's numbers and abilities as display rows. A field the catalog lacks gives no row. */
export function describeUnit(resolved: ResolvedUnit): UnitDescription {
  const u = resolved.unit;
  const facts: UnitFact[] = [];
  if (u.cost !== undefined) facts.push({ label: "Cost", value: formatCost(u.cost) });
  if (u.combatHitsOn !== undefined && u.combatDieCount !== 0) {
    facts.push({ label: "Combat", value: dice(u.combatHitsOn, u.combatDieCount) });
  }
  if (u.moveValue !== undefined) facts.push({ label: "Move", value: String(u.moveValue) });
  if (u.capacityValue !== undefined) facts.push({ label: "Capacity", value: String(u.capacityValue) });
  if (u.productionValue !== undefined) {
    facts.push({ label: "Production", value: u.productionValue });
  }

  const keywords: string[] = [];
  if (u.sustainDamage) keywords.push("Sustain damage");
  if (u.bombardHitsOn !== undefined) {
    keywords.push(`Bombardment ${dice(u.bombardHitsOn, u.bombardDieCount)}`);
  }
  if (u.afbHitsOn !== undefined) {
    keywords.push(`Anti-fighter barrage ${dice(u.afbHitsOn, u.afbDieCount)}`);
  }
  if (u.spaceCannonHitsOn !== undefined) {
    keywords.push(
      `Space cannon ${dice(u.spaceCannonHitsOn, u.spaceCannonDieCount)}${u.deepSpaceCannon ? " (deep)" : ""}`,
    );
  }
  if (u.planetaryShield) keywords.push("Planetary shield");
  if (u.disablesPlanetaryShield) keywords.push("Disables planetary shield");

  return {
    id: u.id,
    name: u.name.trim(),
    subtitle: u.subtitle,
    typeLabel: TYPE_LABELS[u.baseType] ?? u.baseType,
    facts,
    keywords,
    text: u.ability ? u.ability.split("\n").filter(Boolean) : [],
    factionSpecific: resolved.factionSpecific,
    upgraded: resolved.upgraded,
  };
}

export interface TechLine {
  id: string;
  name: string;
  description: string;
}

export interface LeaderLine {
  id: string;
  type: "agent" | "commander" | "hero";
  name: string;
  title?: string;
  abilityName?: string;
  window?: string;
  text?: string;
  unlock?: string;
}

export interface FactionInfo {
  id: string;
  name: string;
  commodities?: number;
  abilities: FactionAbilityRecord[];
  promissoryNotes: PromissoryNoteRecord[];
  flagship?: UnitDescription;
  mech?: UnitDescription;
  startingTech: TechLine[];
  factionTech: TechLine[];
  leaders: LeaderLine[];
  /** The faction's printed Thunder's Edge breakthrough, when the catalog has one. */
  breakthrough?: { id: string; name: string; synergy: string; text?: string };
}

const techLine = (id: string): TechLine => {
  const meta = findTechnologyMeta(id);
  return { id, name: meta?.name ?? id, description: meta?.description ?? "" };
};

function breakthroughLine(factionId: string): FactionInfo["breakthrough"] {
  const card = findBreakthroughForFaction(factionId);
  if (!card) return undefined;
  return { id: card.id, name: card.name, synergy: synergyText(card), text: card.text };
}

const LEADER_ORDER = ["agent", "commander", "hero"];

/**
 * Everything the catalog says about a faction, or `undefined` for a faction it does not know (the
 * caller then shows a "no information" state rather than guessing).
 */
export function describeFaction(
  factionKey: string | null | undefined,
  ownedTechs: readonly string[] = [],
): FactionInfo | undefined {
  const faction = findFactionRecord(factionKey);
  if (!faction) return undefined;
  const unitOf = (type: string) => {
    const resolved = resolveUnit(type, faction.id, ownedTechs);
    return resolved?.factionSpecific ? describeUnit(resolved) : undefined;
  };
  const leaders = faction.leaders
    .map((id) => LEADERS[id])
    .filter((l): l is LeaderRecord => !!l)
    .sort((a, b) => LEADER_ORDER.indexOf(a.type) - LEADER_ORDER.indexOf(b.type))
    .map<LeaderLine>((l) => ({
      id: l.id,
      type: l.type as LeaderLine["type"],
      name: l.name,
      title: l.title,
      abilityName: l.abilityName,
      window: l.abilityWindow,
      text: l.abilityText,
      unlock: l.unlockCondition,
    }));
  return {
    id: faction.id,
    name: faction.name,
    commodities: faction.commodities,
    abilities: faction.abilities
      .map((id) => FACTION_ABILITIES[id])
      .filter((a): a is FactionAbilityRecord => !!a),
    promissoryNotes: faction.promissoryNotes
      .map((id) => PROMISSORY_NOTES[id])
      .filter((n): n is PromissoryNoteRecord => !!n),
    flagship: unitOf("flagship"),
    mech: unitOf("mech"),
    startingTech: faction.startingTech.map(techLine),
    factionTech: faction.factionTech.map(techLine),
    leaders,
    breakthrough: breakthroughLine(faction.id),
  };
}

/** The leader's state as the server reports it for this seat ("Locked", "Readied", ...), lowercased; `undefined` when absent. */
export function leaderStatus(
  leaders: Record<string, string> | undefined,
  leaderId: string,
): string | undefined {
  const status = leaders?.[leaderId];
  return status ? status.toLowerCase() : undefined;
}

export const IN_SCOPE_FACTION_IDS = ["sol", "hacan", "letnev", "xxcha", "jolnar", "l1z1x"] as const;
