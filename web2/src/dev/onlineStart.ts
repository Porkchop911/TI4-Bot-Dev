import type { TableRequest } from "../session/table";

/**
 * The table that `?online` asks the server for.
 * `?online&players=6&seed=3&name=Ada&map=random&wait=1`: each part can be left out. With `fill=random` it is the quick start: a game of eight, this
 * page has one seat, the other seats decide at random, and the game starts at once.
 */
export interface OnlineStart {
  /** The name of the player of this page. Without it: the name that this browser kept. */
  nickname?: string;
  /** Without it the server chooses one. */
  seed?: number;
  /** A map of the server (`GET /api/maps`), or "random". Without it: the default map of the count. */
  map?: string;
  /** The number of seats, 2 to 8. Without it: what this browser used last. */
  players?: number;
  fill?: "random";
  /** The wait of the seats that the server plays, in seconds (0 to 30). Without it: the server's. */
  wait?: number;
}

export const DEFAULT_NICKNAME = "Player";
/** The seats of a table when the browser made none before, as the lobby of `web/`. */
export const DEFAULT_PLAYERS = 4;
/** The seats of the quick start. */
const QUICK_PLAYERS = 8;

const seats = (value: number) => Number.isInteger(value) && value >= 2 && value <= 8;

export function onlineStart(params: URLSearchParams): OnlineStart {
  const seed = Number(params.get("seed") || Number.NaN);
  const players = Number(params.get("players") || Number.NaN);
  const map = params.get("map");
  const wait = Number(params.get("wait") || Number.NaN);
  const nickname = params.get("name")?.trim();
  return {
    ...(nickname ? { nickname } : {}),
    ...(Number.isSafeInteger(seed) && seed >= 0 ? { seed } : {}),
    ...(map ? { map } : {}),
    ...(seats(players) ? { players } : {}),
    ...(params.get("fill") === "random" ? { fill: "random" as const } : {}),
    ...(wait >= 0 && wait <= 30 ? { wait } : {}),
  };
}

/** The address that starts such a table. */
export function onlineAddress(start: OnlineStart = {}): string {
  const params = new URLSearchParams();
  if (start.fill) {
    params.set("fill", start.fill);
  }
  if (start.players !== undefined) {
    params.set("players", String(start.players));
  }
  if (start.seed !== undefined) {
    params.set("seed", String(start.seed));
  }
  if (start.nickname) {
    params.set("name", start.nickname);
  }
  if (start.map) {
    params.set("map", start.map);
  }
  if (start.wait !== undefined) {
    params.set("wait", String(start.wait));
  }
  const rest = params.toString();
  return rest ? `?online&${rest}` : "?online";
}

/** The address of a table, for a tab that is the player `seat`; without it, for a visitor. */
export function tableAddress(gameId: string, seat?: string): string {
  const game = `?online=${encodeURIComponent(gameId)}`;
  return seat === undefined ? game : `${game}&seat=${encodeURIComponent(seat)}`;
}

/** What the table is made with. `kept` is what the browser kept of the tables before. */
export function tableRequest(
  start: OnlineStart,
  kept: { nickname: string | null; players: number | null } = { nickname: null, players: null },
): TableRequest {
  return {
    playerCount:
      start.players ??
      (start.fill
        ? QUICK_PLAYERS
        : kept.players !== null && seats(kept.players)
          ? kept.players
          : DEFAULT_PLAYERS),
    nickname: start.nickname ?? (kept.nickname?.trim() || DEFAULT_NICKNAME),
    ...(start.seed === undefined ? {} : { seed: start.seed }),
    ...(start.map === undefined ? {} : { map: start.map }),
    ...(start.fill ? { fill: start.fill } : {}),
    ...(start.wait === undefined ? {} : { waitMs: Math.round(start.wait * 1000) }),
  };
}
