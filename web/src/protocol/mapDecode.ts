import type {
  BoardTileView,
  MapChoice,
  MapChoiceDto,
  MapPreviewDto,
  MapSeatPreview,
  MapTemplateSummary,
} from "./types.ts";

function fail(message: string): never {
  throw new Error(`Invalid map response: ${message}`);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

const count = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0;

const text = (value: unknown, max = 128): value is string =>
  typeof value === "string" && value.length > 0 && value.length <= max;

/** The public description of a lobby's map choice. */
export function decodeMapChoice(value: unknown): MapChoiceDto {
  if (
    !isRecord(value) ||
    (value.kind !== "template" && value.kind !== "random") ||
    (value.alias !== undefined && !text(value.alias)) ||
    (value.name !== undefined && !text(value.name)) ||
    (value.author !== undefined && typeof value.author !== "string") ||
    !count(value.systems) ||
    typeof value.hyperlanes !== "boolean" ||
    typeof value.recommended !== "boolean"
  )
    fail("map choice");
  if ((value.kind === "template") !== (value.alias !== undefined)) fail("map choice alias");
  return {
    kind: value.kind,
    ...(value.alias === undefined ? {} : { alias: value.alias as string }),
    ...(value.name === undefined ? {} : { name: value.name as string }),
    ...(value.author === undefined ? {} : { author: value.author as string }),
    systems: value.systems,
    hyperlanes: value.hyperlanes,
    recommended: value.recommended,
  };
}

/** `GET /api/maps`; entries that do not build are dropped, so the picker only offers real maps. */
export function decodeMapList(value: unknown): MapTemplateSummary[] {
  if (!Array.isArray(value)) fail("map list");
  return value
    .map((entry): MapTemplateSummary => {
      if (
        !isRecord(entry) ||
        !text(entry.alias) ||
        typeof entry.author !== "string" ||
        (entry.name !== undefined && !text(entry.name)) ||
        !count(entry.player_count)
      )
        fail("map list entry");
      return {
        alias: entry.alias,
        ...(entry.name === undefined ? {} : { name: entry.name as string }),
        author: entry.author,
        player_count: entry.player_count,
        buildable: entry.buildable === true,
        systems: count(entry.systems) ? entry.systems : 0,
        hyperlanes: entry.hyperlanes === true,
        recommended: entry.recommended === true,
      };
    })
    .filter((entry) => entry.buildable);
}

function decodeTile(entry: unknown): BoardTileView {
  if (
    !isRecord(entry) ||
    !text(entry.system_id) ||
    typeof entry.label !== "string" ||
    typeof entry.q !== "number" ||
    typeof entry.r !== "number" ||
    !Number.isInteger(entry.q) ||
    !Number.isInteger(entry.r)
  )
    fail("map tile");
  return entry as unknown as BoardTileView;
}

function decodeSeat(entry: unknown): MapSeatPreview {
  if (
    !isRecord(entry) ||
    !count(entry.seat) ||
    !text(entry.home_system_id)
  )
    fail("map seat");
  return {
    seat: entry.seat,
    home_system_id: entry.home_system_id,
  };
}

export function decodeMapPreview(value: unknown): MapPreviewDto {
  if (!isRecord(value) || !isRecord(value.choice) || !count(value.player_count))
    fail("map preview");
  if (!Array.isArray(value.tiles) || value.tiles.length > 400) fail("map preview tiles");
  if (!Array.isArray(value.seats) || value.seats.length > 8) fail("map preview seats");
  const choice: MapChoice =
    value.choice.kind === "template" && text(value.choice.alias)
      ? { kind: "template", alias: value.choice.alias }
      : value.choice.kind === "random"
        ? { kind: "random" }
        : fail("map preview choice");
  return {
    choice,
    player_count: value.player_count,
    tiles: value.tiles.map(decodeTile),
    seats: value.seats.map(decodeSeat),
  };
}
