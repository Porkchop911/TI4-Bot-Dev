import type { Page, WebSocketRoute } from "@playwright/test";
import type { LobbyDto } from "../../../src/protocol/types";
import { actor, galleryPlayers } from "./fixtures";
import { buildSnapshot, GAME_ID, SESSION, type MockGameOptions } from "./mockGame";

/**
 * A mocked game whose "server" can be taken away and brought back, like a phone that slept: while
 * `down` every HTTP request is aborted (what `TypeError: Failed to fetch` is made of) and every
 * websocket closes at once. `snapshot` can be replaced meanwhile to model what happened while the
 * player was away. Counts what the page tried, so tests can assert on the retry behaviour.
 */
export interface ResumableGame {
  /** All HTTP requests abort and websockets close while true. */
  down: boolean;
  /** Fail this many more websocket connects, then accept again. */
  failSockets: number;
  /** Fail this many more snapshot GETs (network abort), then answer again. */
  failSnapshots: number;
  /** Answer snapshot GETs with this status instead (e.g. 404 for a game that is gone, 502 for a proxy). */
  snapshotStatus: number | null;
  /** The lobby join/GET answer with this status instead (404: game gone). */
  lobbyStatus: number | null;
  options: MockGameOptions;
  version: number;
  counts: { sockets: number; snapshots: number; lobby: number; heartbeats: number; batches: number };
  /** The currently open mocked websocket, if any. */
  socket: WebSocketRoute | null;
  /** Server-side close of the open socket (the tunnel dropped). */
  dropSocket: () => void;
  /** Replace what the server knows (e.g. another seat acted: new version, new choice). */
  setServerState: (options: MockGameOptions) => void;
  /** Answers for POST /batches; default: network failure (the request is lost). */
  onBatch: ((body: unknown) => { status: number; json?: unknown } | "abort") | null;
}

export async function openResumableGame(
  page: Page,
  options: MockGameOptions = {},
): Promise<ResumableGame> {
  const seat = options.seat ?? actor;
  const game: ResumableGame = {
    down: false,
    failSockets: 0,
    failSnapshots: 0,
    snapshotStatus: null,
    lobbyStatus: null,
    options,
    version: options.version ?? 40,
    counts: { sockets: 0, snapshots: 0, lobby: 0, heartbeats: 0, batches: 0 },
    socket: null,
    dropSocket: () => game.socket?.close({ code: 1006, reason: "tunnel dropped" }),
    setServerState: (next) => {
      game.options = next;
    },
    onBatch: null,
  };
  const current = () => buildSnapshot({ ...game.options, version: game.options.version ?? game.version });
  const lobby = (): LobbyDto => ({
    game_id: GAME_ID,
    phase: "running",
    lobby_version: 1,
    host_player_id: seat,
    slots: (game.options.players ?? galleryPlayers).map((p, index) => ({
      slot_id: `slot-${index + 1}`,
      position: index + 1,
      occupant: p.id,
      nickname: p.id === seat ? "You" : `Player ${index + 1}`,
      ready: true,
      connected: true,
      can_take_over: false,
    })),
  });

  await page.route(`**/api/games/${GAME_ID}/**`, async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (game.down) return route.abort("failed");
    if (path.endsWith("/lobby/heartbeat")) {
      game.counts.heartbeats++;
      return route.fulfill({ json: {} });
    }
    if (path.endsWith("/lobby/join") || path.endsWith("/lobby")) {
      game.counts.lobby++;
      if (game.lobbyStatus !== null)
        return route.fulfill({ status: game.lobbyStatus, body: game.lobbyStatus === 404 ? "game not found" : "" });
      return path.endsWith("/join")
        ? route.fulfill({ json: { player_session: SESSION, player: { id: seat }, lobby: lobby() } })
        : route.fulfill({ json: lobby() });
    }
    if (path.endsWith("/snapshot")) {
      game.counts.snapshots++;
      if (game.failSnapshots > 0) {
        game.failSnapshots--;
        return route.abort("failed");
      }
      if (game.snapshotStatus !== null)
        return route.fulfill({
          status: game.snapshotStatus,
          body: game.snapshotStatus === 404 ? "game not found" : "",
        });
      return route.fulfill({ json: current() });
    }
    if (path.endsWith("/turn-redo")) return route.fulfill({ json: { status: null } });
    if (path.endsWith("/batches")) {
      game.counts.batches++;
      const answer = game.onBatch?.(route.request().postDataJSON()) ?? "abort";
      if (answer === "abort") return route.abort("failed");
      return route.fulfill({ status: answer.status, json: answer.json ?? {} });
    }
    return route.fallback();
  });

  await page.routeWebSocket(`**/ws/games/${GAME_ID}`, (socket) => {
    game.counts.sockets++;
    if (game.down || game.failSockets > 0) {
      if (game.failSockets > 0) game.failSockets--;
      socket.close({ code: 1006, reason: "unreachable" });
      return;
    }
    game.socket = socket;
    socket.onMessage((data) => {
      const message = JSON.parse(String(data)) as { type: string };
      if (message.type === "subscribe") socket.send(JSON.stringify(current()));
    });
    socket.onClose(() => {
      if (game.socket === socket) game.socket = null;
    });
  });

  await page.goto("/");
  await page.evaluate(
    ([id, credential]) => sessionStorage.setItem(`ti4.player-session:${id}`, credential),
    [GAME_ID, SESSION],
  );
  await page.goto(`/games/${GAME_ID}`);
  return game;
}

/**
 * Models a phone that slept: the page was hidden, `awayMs` passed on the clock (timers are
 * throttled, so each interval fires at most once), then it became visible again.
 * Needs `page.clock.install()` before the page loaded.
 */
export async function sleepAndWake(page: Page, awayMs: number, afterHidden?: () => void | Promise<void>) {
  const setVisibility = (state: "hidden" | "visible") =>
    page.evaluate((value) => {
      Object.defineProperty(document, "visibilityState", { configurable: true, get: () => value });
      Object.defineProperty(document, "hidden", { configurable: true, get: () => value === "hidden" });
      document.dispatchEvent(new Event("visibilitychange"));
    }, state);
  await setVisibility("hidden");
  await afterHidden?.();
  await page.clock.fastForward(awayMs);
  await setVisibility("visible");
}
