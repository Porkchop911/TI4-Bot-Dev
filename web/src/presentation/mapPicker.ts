import type {
  BoardTileView,
  MapChoice,
  MapChoiceView,
  SeatPreview,
  MapTemplateSummary,
} from "../protocol/types.ts";

/** What a picker card stands for; the server decides which templates exist, never this file. */
export interface MapCardModel {
  key: string;
  choice: MapChoice;
  title: string;
  detail: string;
  /** Short honest labels such as "Hyperlanes" or "Meant for in-person play". */
  badges: string[];
  recommended: boolean;
  selected: boolean;
}

const WORDS: [suffix: string, label: string][] = [
  ["Standard", "Standard"],
  ["BeMyNeighbor", "Be my neighbor"],
  ["Wekkerdraft", "Wekkerdraft"],
  ["AlternateLayout", "Alternate layout"],
  ["Hyperlanes", "Hyperlanes"],
];

/** `6pStandardNucleus` becomes "Standard + Nucleus"; the player count is already on screen. */
export function templateTitle(alias: string): string {
  const base = alias
    .replace(/^\d+p/, "")
    .replace(/InPerson/, "")
    .replace(/Nucleus$/, "");
  const hit = WORDS.find(([suffix]) => base === suffix);
  const name = hit ? hit[1] : base.replace(/([a-z])([A-Z0-9])/g, "$1 $2") || alias;
  return /Nucleus$/.test(alias) ? `${name} + Nucleus` : name;
}

/**
 * Labels that tell the host what a layout is, so nothing is picked by accident. The server's
 * hyperlane flag is the authority for hyperlanes; the alias only says how the layout is meant to
 * be played.
 */
export function templateBadges(alias: string, summary?: { hyperlanes: boolean }): string[] {
  const badges: string[] = [];
  if (summary?.hyperlanes) badges.push("Hyperlanes");
  if (/Nucleus/.test(alias)) badges.push("Nucleus (no special rules yet)");
  if (/InPerson/.test(alias)) badges.push("Meant for in-person play");
  return badges;
}

export function sameChoice(a: MapChoice | MapChoiceView | undefined, b: MapChoice | undefined) {
  if (!a || !b) return false;
  return (
    a.kind === b.kind && (a.kind !== "template" || a.alias === (b as { alias?: string }).alias)
  );
}

export function choiceOf(dto: MapChoiceView | undefined): MapChoice | undefined {
  if (!dto) return undefined;
  return dto.kind === "template" && dto.alias
    ? { kind: "template", alias: dto.alias }
    : dto.kind === "random"
      ? { kind: "random" }
      : undefined;
}

/** The templates the server listed, then Random last. Random is always offered. */
export function mapCards(
  templates: readonly MapTemplateSummary[],
  current: MapChoiceView | undefined,
): MapCardModel[] {
  const selected = choiceOf(current);
  const cards: MapCardModel[] = templates.map((t) => {
    const choice: MapChoice = { kind: "template", alias: t.alias };
    return {
      key: t.alias,
      choice,
      title: t.name ?? templateTitle(t.alias),
      detail: `${t.systems} systems${t.author ? ` · ${t.author}` : ""}`,
      badges: templateBadges(t.alias, t),
      recommended: t.recommended,
      selected: sameChoice(selected, choice),
    };
  });
  const random: MapChoice = { kind: "random" };
  cards.push({
    key: "random",
    choice: random,
    title: "Random",
    detail: "A new random galaxy; re-roll until you like it",
    badges: [],
    recommended: false,
    selected: sameChoice(selected, random),
  });
  return cards;
}

/** One line under the preview: `6 players · Standard by Someone · 37 systems · no hyperlanes`. */
export function mapSummaryLine(map: MapChoiceView): string {
  const count = map.kind === "random" ? null : /^(\d+)p/.exec(map.alias ?? "")?.[1];
  const players = count ? `${count} players · ` : "";
  const name =
    map.kind === "random"
      ? "Random map"
      : (map.name ?? (map.alias ? templateTitle(map.alias) : "Map"));
  const by = map.author ? ` by ${map.author}` : "";
  const systems = map.systems > 0 ? ` · ${map.systems} systems` : "";
  return `${players}${name}${by}${systems} · ${map.hyperlanes ? "with hyperlanes" : "no hyperlanes"}`;
}

/** The short name for the lobby row. */
export function mapName(map: MapChoiceView): string {
  return map.kind === "random" ? "Random" : (map.name ?? map.alias ?? "Map");
}

export type TileKind =
  "home" | "mecatol" | "hyperlane" | "anomaly" | "wormhole" | "planets" | "empty";

/** Colour class of a tile for the pictures; homes win, then the rarer features. */
export function tileKind(tile: BoardTileView, homeIds: ReadonlySet<string>): TileKind {
  if (homeIds.has(tile.system_id)) return "home";
  if (tile.system_id === "18") return "mecatol";
  if (tile.hyperlane) return "hyperlane";
  if ((tile.anomalies?.length ?? 0) > 0) return "anomaly";
  if ((tile.wormholes?.length ?? 0) > 0) return "wormhole";
  if ((tile.planets?.length ?? 0) > 0) return "planets";
  return "empty";
}

/** Main-map tiles only; the Nexus and Fracture sit off the map. */
export function mainTiles(tiles: readonly BoardTileView[]): BoardTileView[] {
  return tiles.filter((tile) => !tile.special_area);
}

export function homeIdSet(seats: readonly SeatPreview[]): Set<string> {
  return new Set(seats.map((seat) => seat.home_system_id));
}

/** Notice for players when the host changes the map while they look at the lobby. */
export function mapChangeNotice(
  before: {
    revision: number | undefined;
    map: MapChoiceView | undefined;
  } | null,
  after: { revision: number | undefined; map: MapChoiceView | undefined },
): string | null {
  if (!before || before.revision === undefined || after.revision === undefined) return null;
  if (before.revision === after.revision) return null;
  return "The host changed the map.";
}
