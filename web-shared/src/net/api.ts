// The HTTP API of the server that both apps call: the addresses, the header of a player session,
// a new table, a join of the lobby and the other calls of a lobby, and whether a game is gone.
// Pure: each call takes the `fetch` to use. The answers are typed on the parts that are read here;
// an app reads the lobby with its own types. No text for a player: an app words a refusal itself.

/** The header that proves to the server which player of a game a call is from. */
export const PLAYER_SESSION_HEADER = "x-ti4-player-session";

/** `fetch`, or what a test gives in its place. */
export type Fetch = typeof fetch;

/** The global `fetch`, called as a function of its own: a browser refuses another `this`. */
const pageFetch: Fetch = (...args) => fetch(...args);

export const GAMES_PATH = "/api/games";
export const SETUP_OPTIONS_PATH = "/api/setup-options";

/** `/api/games/<id>`. */
export const gamePath = (gameId: string): string => `${GAMES_PATH}/${encodeURIComponent(gameId)}`;

/** `/api/games/<id>/lobby`, or with `route` a call of it: `/api/games/<id>/lobby/<route>`. */
export const lobbyPath = (gameId: string, route?: string): string =>
  `${gamePath(gameId)}/lobby${route ? `/${route}` : ""}`;

/** The maps that a table of `players` seats can play. */
export const mapsPath = (players: number): string => `/api/maps?player_count=${players}`;

/** The socket of a table (its lobby, also while its game runs). */
export const tableSocketPath = (gameId: string): string =>
  `/ws/tables/${encodeURIComponent(gameId)}`;

/** The header of a player session; none without one (a visitor, or a call as a new player). */
export const sessionHeaders = (session?: string | null): Record<string, string> =>
  session ? { [PLAYER_SESSION_HEADER]: session } : {};

export interface PostOptions {
  /** The player session of the call. */
  session?: string | null;
  signal?: AbortSignal;
}

/** A POST with a JSON body, or with no body (and no content type) when `body` is undefined. */
export function postJson(
  url: string,
  body: unknown,
  options: PostOptions = {},
  fetcher: Fetch = pageFetch,
): Promise<Response> {
  return fetcher(url, {
    method: "POST",
    headers: {
      ...(body === undefined ? {} : { "content-type": "application/json" }),
      ...sessionHeaders(options.session),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    ...(options.signal ? { signal: options.signal } : {}),
  });
}

/** A call of the lobby of a game: `POST /api/games/<id>/lobby/<route>`. */
export const lobbyPost = (
  gameId: string,
  route: string,
  body: unknown,
  options: PostOptions = {},
  fetcher: Fetch = pageFetch,
): Promise<Response> => postJson(lobbyPath(gameId, route), body, options, fetcher);

/** What `POST /api/games` and `…/lobby/join` answer: the parts that are read here. */
export interface Admission<Lobby = unknown> {
  /** Of a new table only. */
  game_id?: string;
  /** Of a new player; a join with a kept session gets none. */
  player_session?: string;
  player: { id: string };
  lobby: Lobby;
}

/**
 * A join of the lobby. `new`: a new player (with no session), or with `session` the player that
 * the session belongs to. `takeover`: the seat of a player who is away, with no session.
 */
export type JoinRequest =
  | { kind: "new"; nickname?: string }
  | { kind: "takeover"; player_id: string; nickname: string };

export const joinLobby = (
  gameId: string,
  request: JoinRequest,
  options: PostOptions = {},
  fetcher: Fetch = pageFetch,
): Promise<Response> => lobbyPost(gameId, "join", request, options, fetcher);

/** The id of the player that a player session of a game belongs to. Null: the game does not know it. */
export async function playerOf(
  gameId: string,
  playerSession: string,
  fetcher: Fetch = pageFetch,
): Promise<string | null> {
  const response = await joinLobby(gameId, { kind: "new" }, { session: playerSession }, fetcher);
  if (!response.ok) {
    return null;
  }
  return ((await response.json()) as Admission).player.id;
}

/** A call that the server refused: its status and the text of its answer. */
export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly body: string,
  ) {
    super(`HTTP ${status}: ${body}`);
    this.name = "ApiError";
  }
}

/** What a new table is made with. */
export interface TableRequest {
  playerCount: number;
  nickname: string;
  /** Without it the server chooses one. */
  seed?: number;
  /** A map of the server by its alias, or "random". Without it: the default map of the count. */
  map?: string;
  /** "random": every other seat is played by the server at random, and the game starts at once. */
  fill?: "random";
  /** The wait of the seats that the server plays, in milliseconds. Without it: the server's. */
  waitMs?: number;
  /** A set of strategy cards, by its id. Without it: the server's. */
  cards?: string;
}

/** The body of `POST /api/games`. */
export const tableBody = (request: TableRequest): Record<string, unknown> => ({
  player_count: request.playerCount,
  nickname: request.nickname,
  ...(request.fill ? { fill_seats: request.fill } : {}),
  ...(request.seed === undefined ? {} : { seed: request.seed }),
  ...(request.waitMs === undefined ? {} : { bot_delay_ms: request.waitMs }),
  ...(request.cards === undefined ? {} : { strategy_card_set: request.cards }),
  ...(request.map === undefined
    ? {}
    : request.map === "random"
      ? { map: "random" }
      : { map_template: request.map }),
});

/**
 * Makes a table on the server; the player of the call is its host. Throws an `ApiError` when the
 * server refuses, and what `fetch` throws when it cannot be reached.
 */
export async function createTable(
  request: TableRequest,
  fetcher: Fetch = pageFetch,
): Promise<Admission> {
  const response = await postJson(GAMES_PATH, tableBody(request), {}, fetcher);
  if (!response.ok) {
    throw new ApiError(response.status, await response.text());
  }
  return response.json();
}

/** The answer of the server for a game that it does not have (deleted, or a wrong address). */
export const isGone = (response: Pick<Response, "status">): boolean => response.status === 404;

/**
 * Whether the server no longer has the game: only its answer "not found" says so. False when the
 * server cannot be reached, since it may come back.
 */
export async function gameGone(gameId: string, fetcher: Fetch = pageFetch): Promise<boolean> {
  try {
    return isGone(await fetcher(lobbyPath(gameId)));
  } catch {
    return false;
  }
}
