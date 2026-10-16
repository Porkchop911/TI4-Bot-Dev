// How ids of the wire protocol are shown. Presentation only: no rule is decided here.
import { type Force, type TechColor, type UnitType, unitName } from "../../model";
import type { Catalog, SessionUpdate, UnitRecord } from "../wire";
import { humanizeId } from "@ti4/shared/content/names.ts";

const SYMBOLS = ["●", "◆", "▲", "■", "✚", "✖", "♣", "★"];
const COLORS = [
  "#8cd5ef",
  "#d69a75",
  "#8dd2b0",
  "#ec7f8f",
  "#b79cf0",
  "#e6c15c",
  "#f0a6d0",
  "#b9d86a",
];
/** The shape and the colour of a seat, by its place at the table. */
export const seatMark = (index: number) => ({
  symbol: SYMBOLS[index % SYMBOLS.length],
  color: COLORS[index % COLORS.length],
});

export { humanizeId };

/**
 * The name that a table calls a faction by, from the catalogue ("Jol-Nar"). Without the
 * catalogue it is the id.
 */
export const factionName = (id: string, catalog: Catalog | null) =>
  catalog?.components.faction_sheets[id]?.name ?? humanizeId(id);
/**
 * Who a seat is, where the page says who the game waits for: the name of its player, as the
 * table says it, and in a game with no table the faction.
 */
export const seatLabel = (update: SessionUpdate, seat: string, catalog: Catalog | null) =>
  update.seats?.[seat]?.nickname ||
  factionName(update.view.players.find((player) => player.id === seat)?.faction ?? seat, catalog);
/** The commodity value of the faction sheet. Null without the catalogue. */
export const commodityValue = (id: string, catalog: Catalog | null): number | null =>
  catalog?.components.faction_sheets[id]?.commodities ?? null;

/**
 * The initiative and the name of a strategy card, from the catalogue. Without the catalogue
 * they are read from the id, which most cards allow: "pok5trade" is card 5, "Trade".
 */
export function strategyCard(
  id: string,
  catalog: Catalog | null,
): { number: number; name: string } {
  const card = catalog?.components.strategy_cards.find((item) => item.id === id);
  if (card) {
    return { number: card.initiative, name: card.name };
  }
  const [, number = "0", name = ""] = /(\d)([a-z]*)$/.exec(id) ?? [];
  return { number: Number(number), name: name ? humanizeId(name) : id };
}

const UNITS: [suffix: string, unit: UnitType][] = [
  ["flagship", "flagship"],
  ["warsun", "warsun"],
  ["dreadnought", "dreadnought"],
  ["carrier", "carrier"],
  ["cruiser", "cruiser"],
  ["destroyer", "destroyer"],
  ["fighter", "fighter"],
  ["mech", "mech"],
  ["infantry", "infantry"],
  ["pds", "pds"],
  ["spacedock", "dock"],
];
/** The display order of unit types. */
export const UNIT_ORDER: UnitType[] = UNITS.map(([, unit]) => unit);
/**
 * A faction's unit has the base type as a part of its id ("sol_carrier", "naaz_mech_space"),
 * and an upgraded unit has a number after it ("cruiser2", "sol_carrier2", "mentak_cruiser3").
 */
export function unitType(id: string): UnitType | null {
  const parts = id.split("_").map((part) => part.replace(/\d$/, ""));
  return UNITS.find(([name]) => parts.includes(name))?.[1] ?? null;
}

const unitRecords = new WeakMap<Catalog, Map<string, UnitRecord>>();
/** The unit card of the catalogue with the id `id`. */
export function unitRecord(id: string, printed: Catalog | null): UnitRecord | null {
  if (!printed) {
    return null;
  }
  let records = unitRecords.get(printed);
  if (!records) {
    records = new Map((printed.records?.units ?? []).map((unit) => [unit.id, unit]));
    unitRecords.set(printed, records);
  }
  return records.get(id) ?? null;
}

/**
 * The name of a unit by its id, as its card prints it: "Cruiser II", "Hel-Titan I",
 * "Z-Grav Eidolon". A printed name has no plural. Without the card it is the name of the base
 * unit, in the plural for `count` other than 1.
 */
export function unitLabel(id: string, printed: Catalog | null, count = 1): string {
  const record = unitRecord(id, printed);
  if (record) {
    return record.name.trim();
  }
  const type = unitType(id);
  return type ? unitName(type, count) : humanizeId(id);
}

/**
 * What units are grouped by where they are counted ("3 Infantry I"): the printed unit when the
 * catalogue has it, so that "Infantry II" stays apart from "Infantry I"; else the base unit.
 */
export const unitKey = (id: string, printed: Catalog | null): string =>
  unitRecord(id, printed) ? id : (unitType(id) ?? id);

/** Unit ids in display order: by base unit, and an upgrade after the unit it upgrades. */
export const byUnitOrder = (a: string, b: string) => {
  const rank = (id: string) => {
    const type = unitType(id);
    return type ? UNIT_ORDER.indexOf(type) : UNIT_ORDER.length;
  };
  return rank(a) - rank(b) || a.localeCompare(b);
};

/**
 * Units counted by the printed unit (by base unit without the catalogue), in display order.
 * A unit that is no known type is left out.
 */
export function forceOf(
  units: { unit_type: string; damaged: boolean }[],
  printed: Catalog | null = null,
): Force {
  const counts = new Map<string, { id: string; count: number; damaged: number }>();
  for (const unit of units) {
    if (unitType(unit.unit_type)) {
      const key = unitKey(unit.unit_type, printed);
      const entry = counts.get(key) ?? { id: unit.unit_type, count: 0, damaged: 0 };
      entry.count++;
      entry.damaged += unit.damaged ? 1 : 0;
      counts.set(key, entry);
    }
  }
  return [...counts.values()]
    .sort((a, b) => byUnitOrder(a.id, b.id))
    .map(({ id, count, damaged }) => ({
      unit: unitType(id)!,
      name: unitLabel(id, printed, count),
      count,
      damaged,
    }));
}

const SHIPS = new Set<UnitType>([
  "flagship",
  "warsun",
  "dreadnought",
  "carrier",
  "cruiser",
  "destroyer",
  "fighter",
]);
export const isShip = (unit: UnitType) => SHIPS.has(unit);
export const isGroundForce = (unit: UnitType) => unit === "infantry" || unit === "mech";

export const ANOMALIES = {
  "asteroid field": "asteroid",
  supernova: "supernova",
  nebula: "nebula",
  "gravity rift": "rift",
} as const;

export const WORMHOLES: Record<string, string> = { ALPHA: "α", BETA: "β", GAMMA: "γ", DELTA: "δ" };

export const TECH_COLORS: Record<string, TechColor> = {
  PROPULSION: "B",
  BIOTIC: "G",
  CYBERNETIC: "Y",
  WARFARE: "R",
};

const PHASES: Record<string, string> = {
  strategy: "Strategy phase",
  action: "Action phase",
  status: "Status phase",
  agenda: "Agenda phase",
};
export const phaseName = (phase: string) => PHASES[phase] ?? humanizeId(phase);

/** The label of an option starts in lower case, as the engine writes it. */
export const sentence = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

/** The name of a reaction by its key alone, for a key that no card in view has. */
export function nameOfKey(key: string, catalog: Catalog | null): string {
  const at = key.indexOf(":");
  if (at < 0) {
    return key;
  }
  const [kind, id] = [key.slice(0, at), key.slice(at + 1)];
  const cards =
    kind === "leader"
      ? catalog?.components.leaders
      : kind === "ability"
        ? catalog?.components.abilities
        : kind === "technology"
          ? catalog?.components.technologies
          : undefined;
  return cards?.find((card) => card.id === id)?.name ?? humanizeId(id);
}
