// The table of a game: who sits at it and how the game is set up, before the game and while it
// runs. `Table` is the seam of the setup, as `Transport` is the seam of a game: the screen of the
// lobby does not know where the table is kept.
import { type Connection, type Socket, backoff, connect, pageSocket } from "./socket";
import {
  type LobbyState,
  type MapChoice,
  type MapPreview,
  type MapSummary,
  PROTOCOL_VERSION,
  type SetupOptions,
  type TableServerMessage,
} from "./wire";

/** A player of a table, and what proves to the server that this page is that player. */
export interface Seat {
  playerId: string;
  playerSession: string;
}

/** The table as one viewer is shown it. */
export interface TableState {
  lobby: LobbyState;
  /** The player of this page at the table. Null: a visitor. */
  viewer: string | null;
  /** The board that the table will play. Null until the server said it. */
  preview: MapPreview | null;
  /** What a host can choose, with the names. Null until the server said it. */
  options: SetupOptions | null;
  /**
   * The table is kept in the page (single player): the page plays the other seats, and nobody
   * else can come to it. It has no address to give away and no name to change.
   */
  local?: true;
}

/** A change of the setup of a table. A part that is left out stays as it is. */
export interface TableSettings {
  players?: number;
  /** A set of strategy cards, by its id. */
  cards?: string;
  /** The wait before each answer of a seat that the server plays, in milliseconds. */
  waitMs?: number;
  /** The id of a scenario, or null for round 1. */
  startFrom?: string | null;
}

export type TableEvent =
  | ({ kind: "table" } & TableState)
  /**
   * The table cannot be reached, and it is tried again. The last table stays in view. The next
   * table ends it, unless "offline" follows that table at once.
   */
  | { kind: "offline" }
  /** A call was refused. The table is as it was. */
  | { kind: "rejected"; reason: string }
  /** The table cannot be shown. Nothing is tried after it. */
  | { kind: "error"; message: string };

export interface Table {
  /** A new listener is told the latest table or error at once, if there is one. */
  subscribe(listener: (event: TableEvent) => void): () => void;
  /**
   * Takes an open seat as a new player. The page that has a player already stays that player:
   * the new one is for another tab. Null when the table refused.
   */
  join(nickname: string): Promise<Seat | null>;
  /**
   * Takes the seat of a person who is away over, as a visitor (`can_take_over` of its slot). The
   * seat keeps its player and gets this name. Null when the table refused.
   */
  takeOver(playerId: string, nickname: string): Promise<Seat | null>;
  /** Gives the seat of this page back. The host cannot leave. */
  leave(): void;
  /** Lets the server play one open seat, or with `fill` every open seat. Host only. */
  addBots(kind: string, fill?: boolean): void;
  /** Removes a seat that the server plays, or another player. Host only. */
  removeSeat(playerId: string): void;
  /**
   * Changes the setup. Host only. The change applies at once, for every player. Of a running
   * game only the wait changes.
   */
  setSettings(settings: TableSettings): void;
  /** Gives the player of this page another name. */
  rename(nickname: string): void;
  /**
   * Chooses the faction of a seat, or with null leaves it to the draw of the start. A player
   * chooses for the own seat, the host for any. A faction that another seat has is refused.
   */
  setFaction(playerId: string, faction: string | null): void;
  /** The maps that a table of `players` seats can play. */
  maps(players: number): Promise<MapSummary[]>;
  /** Chooses the map. Host only. A map is for one number of seats. */
  setMap(map: MapChoice): void;
  /** Starts the game. Host only, with every seat filled. */
  start(): void;
  close(): void;
}

/** What the table says of a game that the server does not have. */
export const TABLE_UNKNOWN = "The server no longer knows this game.";

export interface ServerTablePlay {
  /** Opens the socket of a table. Without it: `/ws/tables/<id>` at the address of the page. */
  open?(gameId: string): Socket;
  retryAfter?(attempt: number): number;
  /** Without it: `fetch` of the page. */
  fetch?: typeof fetch;
}

const tableSocket = (gameId: string): Socket =>
  pageSocket(`/ws/tables/${encodeURIComponent(gameId)}`);

/** What `POST /api/games` and `…/lobby/join` answer: the part that the page keeps. */
interface Admission {
  game_id?: string;
  player_session?: string;
  player: { id: string };
  lobby: LobbyState;
}

async function refusal(response: Response): Promise<string> {
  const text = (await response.text()).trim();
  return text || `The server refused (${response.status}).`;
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
}

/** Makes a table on the server. The player of this page is its host. */
export async function createTable(
  request: TableRequest,
  fetcher: typeof fetch = fetch,
): Promise<Seat & { gameId: string }> {
  const response = await fetcher("/api/games", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      player_count: request.playerCount,
      nickname: request.nickname,
      ...(request.fill ? { fill_seats: request.fill } : {}),
      ...(request.seed === undefined ? {} : { seed: request.seed }),
      ...(request.waitMs === undefined ? {} : { bot_delay_ms: request.waitMs }),
      ...(request.map === undefined
        ? {}
        : request.map === "random"
          ? { map: "random" }
          : { map_template: request.map }),
    }),
  });
  if (!response.ok) {
    throw new Error(
      `The server did not make a table (${response.status}). ${await response.text()}`,
    );
  }
  const made: Admission = await response.json();
  return { gameId: made.game_id!, playerId: made.player.id, playerSession: made.player_session! };
}

/** The id of the player that a player session of a game belongs to. Null: the game does not know it. */
export async function playerOf(
  gameId: string,
  playerSession: string,
  fetcher: typeof fetch = fetch,
): Promise<string | null> {
  const response = await fetcher(`/api/games/${encodeURIComponent(gameId)}/lobby/join`, {
    method: "POST",
    headers: { "content-type": "application/json", "x-ti4-player-session": playerSession },
    body: JSON.stringify({ kind: "new" }),
  });
  if (!response.ok) {
    return null;
  }
  return ((await response.json()) as Admission).player.id;
}

/**
 * The table of a game on the server: the socket of the table says how it is, and the calls of
 * the lobby change it. `seat` is the player of this page; null is a visitor.
 */
export function serverTable(gameId: string, seat: Seat | null, play: ServerTablePlay = {}): Table {
  const listeners = new Set<(event: TableEvent) => void>();
  const open = play.open ?? tableSocket;
  const retryAfter = play.retryAfter ?? backoff;
  // Called as a function of its own: `fetch` of a browser refuses another `this`.
  const fetcher: typeof fetch = play.fetch ?? ((...args) => fetch(...args));
  const api = `/api/games/${encodeURIComponent(gameId)}/lobby`;
  /** The lobby and the viewer as the server told them last. */
  let latest: { lobby: LobbyState; viewer: string | null } | null = null;
  let preview: MapPreview | null = null;
  /** The `map_revision` of the preview that is there or asked for. */
  let previewed: number | null = null;
  let options: SetupOptions | null = null;
  let failed: string | null = null;
  let offline = false;
  let closed = false;
  let attempt = 0;
  let link: Connection | null = null;

  const say = (event: TableEvent) => {
    for (const listener of listeners) {
      listener(event);
    }
  };
  const tell = () => {
    if (latest) {
      say({ kind: "table", ...latest, preview, options });
      // A table ends the offline state for a listener, and only a table of the socket says that
      // the socket is back: the answer of a call or the board can come while it is away.
      if (offline) {
        say({ kind: "offline" });
      }
    }
  };
  const fail = (message: string) => {
    failed = message;
    link?.close();
    say({ kind: "error", message });
  };

  const call = (route: string, body: unknown, session = seat?.playerSession) =>
    fetcher(`${api}/${route}`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        ...(session ? { "x-ti4-player-session": session } : {}),
      },
      body: JSON.stringify(body),
    });

  /**
   * The setup options are the same for every table of a server. They are asked when the socket
   * opens, until they are there: a call that failed is made again with the next socket.
   */
  const askOptions = () => {
    if (options) {
      return;
    }
    fetcher("/api/setup-options")
      .then(async (response) => {
        if (closed || !response.ok) {
          return;
        }
        options = await response.json();
        tell();
      })
      .catch(() => {});
  };

  const askPreview = (revision: number) => {
    previewed = revision;
    fetcher(`${api}/map-preview`)
      .then(async (response) => {
        if (closed || previewed !== revision || !response.ok) {
          return;
        }
        preview = await response.json();
        tell();
      })
      // The next change of the table asks again.
      .catch(() => {
        if (previewed === revision) {
          previewed = null;
        }
      });
  };

  /**
   * Takes a view of the lobby. Of two views the one with the higher `lobby_version` is kept.
   * The socket also tells who is connected, which changes no version: its view of the same
   * version replaces the one before, the answer of a call does not.
   */
  const take = (lobby: LobbyState, viewer: string | null, pushed: boolean) => {
    if (closed || failed !== null) {
      return;
    }
    const known = latest?.lobby.lobby_version ?? -1;
    if (lobby.lobby_version < known || (lobby.lobby_version === known && !pushed)) {
      return;
    }
    latest = { lobby, viewer };
    // A running table has its board too: a visitor is shown it before the game.
    if (previewed !== lobby.map_revision) {
      askPreview(lobby.map_revision);
    }
    tell();
  };

  /** A call that changes the table. A refusal is told with its reason, and the table stays. */
  const change = async (route: string, body: unknown, send = call) => {
    try {
      const response = await send(route, body);
      if (!response.ok) {
        return say({ kind: "rejected", reason: await refusal(response) });
      }
      // The answer is the lobby view; the viewer is who it was.
      take(await response.json(), latest?.viewer ?? seat?.playerId ?? null, false);
    } catch {
      say({ kind: "rejected", reason: "The server cannot be reached. Nothing was changed." });
    }
  };

  const receive = (text: string) => {
    let message: TableServerMessage;
    try {
      message = JSON.parse(text);
    } catch {
      return fail("The server sent a message that this page cannot read.");
    }
    if (message.protocol_version !== PROTOCOL_VERSION) {
      return fail(
        `The server speaks protocol ${message.protocol_version} and this page protocol ${PROTOCOL_VERSION}. Reload the page.`,
      );
    }
    switch (message.type) {
      case "table":
        attempt = 0;
        offline = false;
        return take(message.lobby, message.viewer ?? null, true);
      case "error":
        if (message.kind === "unauthorized" && seat) {
          // The table does not know the player of this page (it left, or it was removed): the
          // page is shown the table as a visitor.
          return void link?.send(
            JSON.stringify({ type: "subscribe", protocol_version: PROTOCOL_VERSION }),
          );
        }
        return latest ? undefined : fail(`The server refused the table: ${message.message}`);
      default:
        return;
    }
  };

  link = connect(() => open(gameId), {
    opened() {
      askOptions();
      link?.send(
        JSON.stringify({
          type: "subscribe",
          protocol_version: PROTOCOL_VERSION,
          ...(seat ? { player_session: seat.playerSession } : {}),
        }),
      );
    },
    // A lobby is silent for minutes: the ping keeps the socket open and the player connected.
    beat: () =>
      void link?.send(JSON.stringify({ type: "ping", protocol_version: PROTOCOL_VERSION })),
    message: receive,
    lost(event) {
      if (!event.opened) {
        // The server refuses the socket of a table that it does not have. Only the answer
        // "not found" says so: a server that cannot be reached may come back.
        fetcher(api).then(
          (response) => {
            if (response.status === 404 && !closed && failed === null) {
              fail(TABLE_UNKNOWN);
            }
          },
          () => {},
        );
      }
      if (!offline) {
        offline = true;
        say({ kind: "offline" });
      }
      return retryAfter(attempt++);
    },
  });

  return {
    subscribe(listener) {
      listeners.add(listener);
      if (failed !== null) {
        listener({ kind: "error", message: failed });
      } else {
        if (latest) {
          listener({ kind: "table", ...latest, preview, options });
        }
        if (offline) {
          listener({ kind: "offline" });
        }
      }
      return () => listeners.delete(listener);
    },
    async join(nickname) {
      try {
        // With no credential: a new player, also when this page has one already.
        const response = await call("join", { kind: "new", nickname }, "");
        if (!response.ok) {
          say({ kind: "rejected", reason: await refusal(response) });
          return null;
        }
        const made: Admission = await response.json();
        take(made.lobby, latest?.viewer ?? null, false);
        return { playerId: made.player.id, playerSession: made.player_session! };
      } catch {
        say({ kind: "rejected", reason: "The server cannot be reached. Nothing was changed." });
        return null;
      }
    },
    async takeOver(playerId, nickname) {
      try {
        // With no credential: the server gives the seat to nobody who has one.
        const response = await call(
          "join",
          { kind: "takeover", player_id: playerId, nickname },
          "",
        );
        if (!response.ok) {
          say({ kind: "rejected", reason: await refusal(response) });
          return null;
        }
        const made: Admission = await response.json();
        take(made.lobby, latest?.viewer ?? null, false);
        return { playerId: made.player.id, playerSession: made.player_session! };
      } catch {
        say({ kind: "rejected", reason: "The server cannot be reached. Nothing was changed." });
        return null;
      }
    },
    leave: () => void change("leave", {}),
    addBots: (kind, fill = false) => void change("bots", fill ? { kind, fill: true } : { kind }),
    removeSeat: (playerId) => void change("remove-bot", { player_id: playerId }),
    setSettings: (settings) =>
      latest?.lobby.phase === "running"
        ? // The wait is the one value of a table that changes after the start: the game has the call.
          void change("settings", { bot_delay_ms: settings.waitMs }, (_, body) =>
            fetcher(`/api/games/${encodeURIComponent(gameId)}/settings`, {
              method: "POST",
              headers: {
                "content-type": "application/json",
                ...(seat ? { "x-ti4-player-session": seat.playerSession } : {}),
              },
              body: JSON.stringify(body),
            }),
          )
        : void change("settings", {
            ...(settings.players === undefined ? {} : { player_count: settings.players }),
            ...(settings.cards === undefined ? {} : { strategy_card_set: settings.cards }),
            ...(settings.waitMs === undefined ? {} : { bot_delay_ms: settings.waitMs }),
            ...(settings.startFrom === undefined ? {} : { start_from: settings.startFrom }),
          }),
    rename: (nickname) => void change("nickname", { nickname }),
    setFaction: (playerId, faction) =>
      void change("seat-faction", { player_id: playerId, faction }),
    async maps(players) {
      const response = await fetcher(`/api/maps?player_count=${players}`);
      if (!response.ok) {
        throw new Error(await refusal(response));
      }
      return response.json();
    },
    setMap: (map) => void change("map", { map }),
    start: () => void change("start", {}),
    close() {
      closed = true;
      link?.close();
      listeners.clear();
    },
  };
}
