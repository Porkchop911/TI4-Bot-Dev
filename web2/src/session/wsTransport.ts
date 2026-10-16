// The game on the server: `crates/ti4-server`, behind a websocket at the address of the page.
import { fleetStanding } from "@ti4/shared/engine/fleet.ts";
import { groundOdds, pageQuery, spaceOdds } from "@ti4/shared/engine/odds.ts";
import { socketLink } from "@ti4/shared/link/socketLink.ts";
import { type Socket, backoff, pageSocket } from "@ti4/shared/net/socket.ts";
import { type MessageCore, messageTransport } from "./messageTransport";
import type { LegacyStaged } from "./seatStore";
import type { Transport } from "./transport";
import {
  type Catalog,
  type FleetCounts,
  type GroundOddsRequest,
  PROTOCOL_VERSION,
  type SpaceOddsRequest,
} from "./wire";

/** Which game of the server, and as whom. */
export interface OnlineGame {
  gameId: string;
  /** What the server gave the player when the game was made or joined. Null: a spectator. */
  playerSession: string | null;
  /** What an older page kept of the staged work of the player in this browser: put once, when the server has none. */
  legacyStaged?: LegacyStaged;
}

export { type Socket, openSocket } from "@ti4/shared/net/socket.ts";

export interface OnlinePlay {
  /** Opens the socket of a game. Without it: `/ws/games/<id>` at the address of the page. */
  open?(gameId: string): Socket;
  /** How long to wait before try `attempt` (from 0) to reach the game again, in ms. */
  retryAfter?(attempt: number): number;
  /**
   * Whether the server has the game. Asked when a socket did not open: the server refuses the
   * socket of a game that it does not have, and a socket cannot say why it did not open.
   * Without it: asked at the address of the page.
   */
  known?(gameId: string): Promise<boolean>;
}

/** What the transport says of a game that the server does not have. Nothing is tried after it. */
export const GAME_UNKNOWN = "The server no longer knows this game.";

/** The server closes a socket with this code when the game was replaced (a restore of the host, an undo of a seat). */
const REPLACED = 4001;

/** The server is at the address of the page, as the page itself. */
const gameSocket = (gameId: string): Socket =>
  pageSocket(`/ws/games/${encodeURIComponent(gameId)}`);

/** Only the answer "not found" says that the game is gone: a server that cannot be reached may come back. */
const pageKnows = (gameId: string): Promise<boolean> =>
  fetch(`/api/games/${encodeURIComponent(gameId)}/map`).then(
    (response) => response.status !== 404,
    () => true,
  );

/**
 * Plays one game of the server (`socketLink`, read by `messageTransport`). The first update is
 * the snapshot that the server sends after the subscription. `catalog` is the one of the engine
 * of this page (`Engine.catalog`): the server sends ids, and the game is played only when both
 * have the same corpus. The odds of a combat are computed by `module`, the engine of this page,
 * on the main thread (`pageQuery`): one instance of it answers every question.
 */
export function createWsTransport(
  engine: { catalog: Catalog | null; module?: WebAssembly.Module },
  game: OnlineGame,
  play: OnlinePlay = {},
): Transport {
  const open = play.open ?? gameSocket;
  const retryAfter = play.retryAfter ?? backoff;
  const known = play.known ?? pageKnows;
  const player = game.playerSession !== null;
  let attempt = 0;
  /** The socket is open: a message reaches the server. */
  let connected = false;
  let closed = false;
  let core: MessageCore | null = null;
  const link = socketLink(() => open(game.gameId), {
    subscribe: () => ({
      type: "subscribe",
      game_id: game.gameId,
      ...(game.playerSession !== null ? { player_session: game.playerSession, staged: true } : {}),
    }),
    stamp: { protocol_version: PROTOCOL_VERSION },
    lost(event) {
      connected = false;
      if (!event.opened) {
        // The server may have refused the socket because it does not have the game.
        void known(game.gameId).then((has) => {
          if (!has && !closed) {
            core?.fail(GAME_UNKNOWN);
          }
        });
      }
      // The server replaced the game for the undo of this side: the game is not lost, and the
      // snapshot of the next subscription is what the undo left. Any other end is a loss.
      const undone = !!core?.undoing() && event.code === REPLACED;
      core?.lost();
      return { wait: event.code === REPLACED ? 0 : retryAfter(attempt++), quiet: undone };
    },
  });
  link.subscribe((event) => {
    if (event.kind === "opened") {
      connected = true;
    }
  });
  core = messageTransport(link, {
    gameId: game.gameId,
    catalog: engine.catalog,
    odds: (request: SpaceOddsRequest) =>
      engine.module
        ? spaceOdds(pageQuery(engine.module), request)
        : Promise.reject(new Error("This page has no engine for the odds.")),
    groundOdds: (request: GroundOddsRequest) =>
      engine.module
        ? groundOdds(pageQuery(engine.module), request)
        : Promise.reject(new Error("This page has no engine for the odds.")),
    fleetStanding: (counts: FleetCounts) =>
      engine.module
        ? fleetStanding(pageQuery(engine.module), counts)
        : Promise.reject(new Error("This page has no engine for the fleet supply.")),
    // The tools of a local game are not on the server.
    capabilities: {},
    player,
    legacyStaged: game.legacyStaged,
    ready: () => connected,
    resync() {
      connected = false;
      link.again();
    },
    refused: (error) =>
      error.kind === "unauthorized"
        ? "The server does not know the player of this page in this game."
        : `The server refused the game: ${error.message}`,
    fatal: () => "The server sent a message that this page cannot read.",
    stagedRefused: "The server did not keep what you staged. It stays in this tab.",
    snapshot() {
      attempt = 0;
    },
    closed() {
      closed = true;
    },
  });
  return core.transport;
}
