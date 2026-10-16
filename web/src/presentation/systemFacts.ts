import { BoardView, ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { humanizeId } from "../protocol/contentCatalog.ts";
import { isDeclineOption } from "./choiceModel.ts";
import { getPlanetDetails } from "./planetSelection.ts";

export interface SystemPlanetFact {
  id: string;
  name: string;
  resources: number | null;
  influence: number | null;
  controlledBy: string | null;
}

export interface SystemFacts {
  systemId: string;
  /** "Mecatol Rex (#18)" or "#14". */
  title: string;
  planets: SystemPlanetFact[];
  /** Units in the space area, e.g. ["2 carriers", "1 fighter"], grouped per owner. */
  ships: { owner: string; summary: string }[];
  commandTokens: string[];
}

const UNIT_BASES: [RegExp, string, string][] = [
  [/dreadnought/, "dreadnought", "dreadnoughts"],
  [/destroyer/, "destroyer", "destroyers"],
  [/carrier/, "carrier", "carriers"],
  [/cruiser/, "cruiser", "cruisers"],
  [/fighter/, "fighter", "fighters"],
  [/flagship/, "flagship", "flagships"],
  [/warsun/, "war sun", "war suns"],
  [/spacedock|dock/, "space dock", "space docks"],
  [/pds/, "PDS", "PDS"],
  [/infantry/, "infantry", "infantry"],
  [/mech/, "mech", "mechs"],
];

/** "sol_carrier2" -> "carrier"; unknown ids are humanized instead of shown raw. */
export function unitNoun(unitType: string, count = 1): string {
  const id = unitType.toLowerCase().replace(/[^a-z]/g, "");
  for (const [re, one, many] of UNIT_BASES) if (re.test(id)) return count === 1 ? one : many;
  return humanizeId(unitType).toLowerCase();
}

/** "2 carriers, 1 fighter" */
export function summarizeUnits(units: { unit_type: string }[]): string {
  const counts = new Map<string, number>();
  for (const u of units) {
    const key = unitNoun(u.unit_type, 1);
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return [...counts]
    .map(([noun, n]) => {
      const plural = UNIT_BASES.find(([, one]) => one === noun)?.[2] ?? noun;
      return `${n} ${n === 1 ? noun : plural}`;
    })
    .join(", ");
}

export function systemTitle(systemId: string, board?: BoardView | null): string {
  const label = board?.map_tiles?.find((t) => t.system_id === systemId)?.label;
  const named = label || (systemId === "18" ? "Mecatol Rex" : "");
  return named && named !== `#${systemId}` ? `${named} (#${systemId})` : `System #${systemId}`;
}

/** The printed tile number: "1" -> "01", "18" -> "18", "82a" -> "82a". */
export function systemTileNumber(systemId: string): string {
  return /^\d$/.test(systemId) ? `0${systemId}` : systemId;
}

function capitalize(text: string): string {
  return text ? text[0].toUpperCase() + text.slice(1) : text;
}

/**
 * What a player calls the system: "Mecatol Rex", "Sol home system", "Gravity rift",
 * "Starpoint/New Albion". Null when the board does not know the tile.
 */
export function systemCommonName(systemId: string, board?: BoardView | null): string | null {
  const tile = board?.map_tiles?.find((t) => t.system_id === systemId);
  const label = tile?.label?.trim() ?? "";
  if (!label || label === systemId || label === `#${systemId}`) {
    return systemId === "18" ? "Mecatol Rex" : null;
  }
  const planets = tile?.planets?.length ?? 0;
  // Home tiles are named "<planets> - <faction>" ("Jord - Sol"); the Nexus "Mallice - Nexus" and
  // the planetless Creuss Gate ("Creuss Gate - Creuss") keep their own name.
  const split = label.match(/^(.+?) - (.+)$/);
  if (split) {
    const [, place, owner] = split;
    if (planets === 0 || /nexus/i.test(owner)) return place;
    return `${owner} home system`;
  }
  // Planetless anomalies and empty tiles read by their kind ("Gravity Rift" -> "Gravity rift").
  if (planets === 0) return capitalize(label.toLowerCase());
  const anomaly = tile?.anomalies?.[0];
  return anomaly ? `${label}, ${anomaly}` : label;
}

/**
 * "Lodor (System 27)", "Sol home system (System 1)", "System 41": the system by name, for dialog
 * titles. A tile the board has no name for falls back to its planets, then to the bare number.
 */
export function systemNamedLabel(systemId: string, board?: BoardView | null): string {
  const bare = `System ${systemId}`;
  const name =
    systemCommonName(systemId, board) ??
    ((board?.map_tiles?.find((t) => t.system_id === systemId)?.planets ?? [])
      .map((planet) => humanizeId(planet.id))
      .join(" / ") ||
      null);
  return name ? `${name} (${bare})` : bare;
}

/** "#18 [Mecatol Rex]", "#01 [Sol home system]", "#41 [Gravity rift]"; "#99" when unknown. */
export function systemTag(systemId: string, board?: BoardView | null): string {
  const name = systemCommonName(systemId, board);
  const number = `#${systemTileNumber(systemId)}`;
  return name ? `${number} [${name}]` : number;
}

export function describeSystem(systemId: string, board?: BoardView | null): SystemFacts {
  const tile = board?.map_tiles?.find((t) => t.system_id === systemId);
  const dyn = board?.systems?.[systemId];
  const planetIds = new Set<string>([
    ...(tile?.planets?.map((p) => p.id) ?? []),
    ...Object.keys(dyn?.planets ?? {}),
  ]);
  const planets = [...planetIds].map((id) => {
    const d = getPlanetDetails(id, board);
    return {
      id,
      name: d.name,
      resources: d.resources,
      influence: d.influence,
      controlledBy: d.controlledBy,
    };
  });
  const byOwner = new Map<string, { unit_type: string }[]>();
  for (const u of dyn?.units ?? []) {
    if (u.planet) continue;
    byOwner.set(u.owner, [...(byOwner.get(u.owner) ?? []), u]);
  }
  return {
    systemId,
    title: systemTitle(systemId, board),
    planets,
    ships: [...byOwner].map(([owner, units]) => ({ owner, summary: summarizeUnits(units) })),
    commandTokens: dyn?.command_tokens ?? [],
  };
}

/** Units and structures standing on one planet, grouped per owner. */
export function planetOccupants(
  systemId: string | null,
  planetId: string,
  board?: BoardView | null,
): { owner: string; summary: string }[] {
  const units = (systemId ? board?.systems?.[systemId]?.units : undefined) ?? [];
  const byOwner = new Map<string, { unit_type: string }[]>();
  for (const u of units) {
    if (u.planet !== planetId) continue;
    byOwner.set(u.owner, [...(byOwner.get(u.owner) ?? []), u]);
  }
  return [...byOwner].map(([owner, list]) => ({ owner, summary: summarizeUnits(list) }));
}

const SYSTEM_PICK_SUBTYPE = /(_pick_system|_choose_system|_recall_token|_pick_recall)$/;

/** The system an option of a system pick stands for: payload.system, else the option id. */
export function systemPickOptionSystem(opt: ChoiceOptionDto): string | null {
  if (isDeclineOption(opt)) return null;
  const p = opt.payload?.system;
  if (typeof p === "string" && p) return p;
  if (typeof p === "number") return String(p);
  return opt.id;
}

/**
 * A single pick among systems whose options are bare system ids (diplomacy, warfare recall,
 * jamming, skilled retreat, ...). Needs the board to confirm that every id is a system.
 */
export function isSystemPickChoice(
  choice: PendingChoiceDto | null,
  board?: BoardView | null,
): boolean {
  if (!choice || !board) return false;
  if (!SYSTEM_PICK_SUBTYPE.test(choice.context?.subtype ?? "")) return false;
  const offered = choice.options.filter((o) => !isDeclineOption(o));
  if (offered.length === 0) return false;
  return offered.every((o) => {
    const id = systemPickOptionSystem(o);
    return Boolean(
      id && (board.systems?.[id] || board.map_tiles?.some((t) => t.system_id === id)),
    );
  });
}
