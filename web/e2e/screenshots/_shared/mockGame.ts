import type { Page, WebSocketRoute } from "@playwright/test";
import {
  PROTOCOL_VERSION,
  type BoardView,
  type ChoiceOptionDto,
  type ClientMessage,
  type GameEvent,
  type GameView,
  type InitialSnapshotMsg,
  type LobbyDto,
  type PlayerView,
  type PublicTurnStatus,
  type TableView,
} from "../../../src/protocol/types";
import { actor, galleryBoard, galleryPlayers } from "./fixtures";

export const GAME_ID = "screenshot-game";
export const SESSION = "screenshot-session";

export interface MockChoice {
  prompt: string;
  context?: Record<string, unknown>;
  options: ChoiceOptionDto[];
  /** Seat that has to decide (default: the viewing seat). */
  player?: string;
  nonce?: string;
  /** Structured decision details (e.g. `{ kind: "turn_menu", ... }` for the turn action bar). */
  details?: Record<string, unknown>;
}

export interface MockGameOptions {
  /** Seat the browser plays as. */
  seat?: string;
  round?: number;
  phase?: string;
  board?: BoardView;
  players?: PlayerView[];
  table?: Partial<TableView>;
  /** Pending decision; omit for a game that is just waiting. */
  choice?: MockChoice | null;
  events?: GameEvent[];
  /** Engine `GameState` fields the page reads (e.g. `exploration_log`); default `{}`. */
  state?: Record<string, unknown>;
  /** Undo/redo position; with log entries that carry decision_count it enables the log Undo buttons. */
  history?: { cursor: number; redo_count: number; generation?: number };
  turnStatus?: PublicTurnStatus;
  view?: Partial<GameView>;
  version?: number;
  /** Watch as a spectator: no seat, no private data, every public action visible. */
  spectator?: boolean;
  /** Lobby nicknames by seat id; seats without an entry keep "You" / "Player N". */
  nicknames?: Record<string, string>;
  /** The lobby host (default: the viewing seat, who then sees the host-only controls). */
  host?: string;
  /** Sees every client message the mocked server does not answer itself; `reply` pushes one back. */
  onClientMessage?: (message: ClientMessage, reply: (message: unknown) => void) => void;
}

export interface MockedGame {
  socket: WebSocketRoute;
  /** Pushes a server message (state_update, pending_choice, event, turn_status ...) to the page. */
  send: (message: unknown) => void;
  snapshot: InitialSnapshotMsg & { type: "initial_snapshot" };
}

export function buildSnapshot(options: MockGameOptions = {}) {
  const seat = options.seat ?? actor;
  const players = options.players ?? galleryPlayers;
  const choice = options.choice;
  const phase = options.phase ?? "action";
  const round = options.round ?? 2;
  const snapshot: InitialSnapshotMsg & { type: "initial_snapshot" } = {
    type: "initial_snapshot",
    protocol_version: PROTOCOL_VERSION,
    game_id: GAME_ID,
    game_version: options.version ?? 40,
    viewer: options.spectator ? { role: "spectator" } : { role: "player", seat },
    state: options.state ?? {},
    galaxy_layout: { version: 1, active_sources: [], placements: [] },
    view: {
      round,
      phase,
      speaker: seat,
      seating_order: players.map((p) => p.id),
      active_player: seat,
      finished: false,
      players,
      board: options.board ?? galleryBoard,
      table: {
        revealed_objectives: [],
        scored_objectives: {},
        unclaimed_strategy_cards: [],
        strategy_card_goods: {},
        laws: {},
        ...options.table,
      },
      ...options.view,
    },
    turn_status: options.turnStatus ??
      (choice
        ? {
            kind: "waiting_for_decision",
            seat: choice.player ?? seat,
            phase,
            round,
            stage: String(choice.context?.subtype ?? "decision"),
          }
        : { kind: "active_turn", player: seat, phase, round }),
    pending_choice: choice
      ? {
          nonce: choice.nonce ?? "screenshot-nonce",
          choice: {
            player: choice.player ?? seat,
            prompt: choice.prompt,
            context: (choice.context ?? { subtype: "decision" }) as never,
            options: choice.options,
            ...(choice.details ? { details: choice.details } : {}),
          },
        }
      : null,
    events: options.events ?? [],
    ...(options.history ? { history: options.history } : {}),
  };
  return snapshot;
}

/**
 * Renders the real app at /games/<id> against a synthetic server: HTTP and websocket are routed
 * in the browser, so no backend is needed. Resolves once the page has subscribed.
 */
export async function openMockedGame(page: Page, options: MockGameOptions = {}): Promise<MockedGame> {
  const seat = options.seat ?? actor;
  const snapshot = buildSnapshot(options);
  const lobby: LobbyDto = {
    game_id: GAME_ID,
    phase: "running",
    lobby_version: 1,
    host_player_id: options.host ?? seat,
    slots: snapshot.view.players.map((p, index) => ({
      slot_id: `slot-${index + 1}`,
      position: index + 1,
      occupant: p.id,
      nickname: options.nicknames?.[p.id] ?? (p.id === seat ? "You" : `Player ${index + 1}`),
      ready: true,
      connected: true,
      can_take_over: false,
    })),
  };
  await page.route(`**/api/games/${GAME_ID}/lobby/join`, (route) =>
    route.fulfill({ json: { player_session: SESSION, player: { id: seat }, lobby } }),
  );
  // The read-only lobby a spectator (no credential) loads.
  await page.route(`**/api/games/${GAME_ID}/lobby`, (route) => route.fulfill({ json: lobby }));
  await page.route(`**/api/games/${GAME_ID}/lobby/heartbeat`, (route) => route.fulfill({ json: {} }));
  await page.route(`**/api/games/${GAME_ID}/snapshot`, (route) => route.fulfill({ json: snapshot }));

  let connected!: (socket: WebSocketRoute) => void;
  const socketReady = new Promise<WebSocketRoute>((resolve) => {
    connected = resolve;
  });
  // Like the real server: a seat's reaction modes live on the server, `set_reaction_mode` changes
  // them and the seat's next state update carries the result (only "never" is listed).
  const reactionModes: Record<string, "never"> = {};
  await page.routeWebSocket(`**/ws/games/${GAME_ID}`, (socket) => {
    socket.onMessage((data) => {
      const message = JSON.parse(String(data)) as ClientMessage;
      if (message.type === "subscribe") {
        socket.send(JSON.stringify(snapshot));
        connected(socket);
      } else if (message.type === "set_reaction_mode" && !options.spectator) {
        if (message.mode === "never") reactionModes[message.card] = "never";
        else delete reactionModes[message.card];
        const { events: _events, ...update } = snapshot;
        socket.send(
          JSON.stringify({ ...update, type: "state_update", reaction_modes: { ...reactionModes } }),
        );
      } else {
        options.onClientMessage?.(message, (reply) => socket.send(JSON.stringify(reply)));
      }
    });
  });
  await page.goto("/");
  if (!options.spectator)
    await page.evaluate(
      ([id, credential]) => sessionStorage.setItem(`ti4.player-session:${id}`, credential),
      [GAME_ID, SESSION],
    );
  await page.goto(`/games/${GAME_ID}`);
  // Without a credential the app shows the lobby first; spectators choose to watch.
  if (options.spectator) await page.getByRole("button", { name: "Watch", exact: true }).click();
  const socket = await socketReady;
  return { socket, snapshot, send: (message) => socket.send(JSON.stringify(message)) };
}
