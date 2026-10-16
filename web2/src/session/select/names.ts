// How ids of the wire protocol are shown. Presentation only: no rule is decided here.
import type { Force, TechColor, UnitType } from "../../model";
import type { Catalog, SessionUpdate } from "../wire";

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

export const titleCase = (text: string) =>
  text.replace(
    /(^|[\s_])([a-z])/g,
    (_, gap: string, letter: string) => `${gap === "_" ? " " : gap}${letter.toUpperCase()}`,
  );

/**
 * The name that a table calls a faction by, from the catalogue ("Jol-Nar"). Without the
 * catalogue it is the id.
 */
export const factionName = (id: string, catalog: Catalog | null) =>
  catalog?.components.faction_sheets?.[id]?.name ?? titleCase(id);
/**
 * Who a seat is, where the page says who the game waits for: the name of its player, as the
 * table says it, and in a game with no table the faction.
 */
export const seatLabel = (update: SessionUpdate, seat: string, catalog: Catalog | null) =>
  update.seats?.[seat]?.nickname ||
  factionName(update.view.players.find((player) => player.id === seat)?.faction ?? seat, catalog);
/** The commodity value of the faction sheet. Null without the catalogue. */
export const commodityValue = (id: string, catalog: Catalog | null): number | null =>
  catalog?.components.faction_sheets?.[id]?.commodities ?? null;

/**
 * The initiative and the name of a strategy card, from the catalogue. Without the catalogue
 * they are read from the id, which most cards allow: "pok5trade" is card 5, "Trade".
 */
export function strategyCard(
  id: string,
  catalog: Catalog | null,
): { number: number; name: string } {
  const card = catalog?.components.strategy_cards?.find((item) => item.id === id);
  if (card) {
    return { number: card.initiative, name: card.name };
  }
  const [, number = "0", name = ""] = /(\d)([a-z]*)$/.exec(id) ?? [];
  return { number: Number(number), name: name ? titleCase(name) : id };
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
 * and an upgraded unit has a 2 after it ("cruiser2", "sol_carrier2").
 */
export function unitType(id: string): UnitType | null {
  const parts = id.split("_").map((part) => part.replace(/2$/, ""));
  return UNITS.find(([name]) => parts.includes(name))?.[1] ?? null;
}

/** Units counted by type, in display order. */
export function forceOf(units: { unit_type: string; damaged: boolean }[]): Force {
  const counts = new Map<UnitType, { count: number; damaged: number }>();
  for (const unit of units) {
    const type = unitType(unit.unit_type);
    if (type) {
      const entry = counts.get(type) ?? { count: 0, damaged: 0 };
      entry.count++;
      entry.damaged += unit.damaged ? 1 : 0;
      counts.set(type, entry);
    }
  }
  return UNIT_ORDER.flatMap((unit) => {
    const entry = counts.get(unit);
    return entry ? [{ unit, ...entry }] : [];
  });
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
export const phaseName = (phase: string) => PHASES[phase] ?? titleCase(phase);

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
  return cards?.find((card) => card.id === id)?.name ?? titleCase(id);
}
