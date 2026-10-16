/**
 * The wire protocol that web2 reads and sends: the types generated from Rust
 * (`web-shared/src/wire/`, `cargo wire-web`), with their doc comments. The server sends a game in
 * `StateUpdateMsg`, the engine of a local game the same messages. Below them are only what web2
 * narrows, and the table of the server, which is not generated.
 */
import type {
  ClientMessage,
  InitialSnapshotMsg,
  MapChoiceView,
  ReactionMode,
  StateUpdateMsg,
} from "@ti4/shared/wire/index.ts";

export * from "@ti4/shared/wire/index.ts";

/** The modes that a seat set, by the key of the reaction. A key that is not there is "always". */
export type ReactionModes = Record<string, Exclude<ReactionMode, "always">>;

/**
 * `initial_snapshot` and `state_update` as one message: a `SessionUpdate` with the version of the
 * game. What only a snapshot has (`content`, `undone`, `events`) is absent from an update.
 */
export type StateMsg = StateUpdateMsg & Pick<InitialSnapshotMsg, "content" | "undone" | "events">;

type WithoutVersion<T> = T extends unknown ? Omit<T, "protocol_version"> : never;

/** A `ClientMessage` before it has the version of the protocol. */
export type ClientRequest = WithoutVersion<ClientMessage>;

// ---- The table of a game on the server, before and while it runs --------------------------------
// The types of `ti4-server` are not generated.

/** `ti4_server::session::registry::PlayerSlotView`: one seat of a table. */
export interface LobbySlot {
  slot_id: string;
  position: number;
  /** The id of the player in the seat. Null: the seat is open. */
  occupant: string | null;
  nickname: string | null;
  connected: boolean;
  can_take_over: boolean;
  /** The server plays the seat: "random" or "smart" (the rule bot). */
  bot?: string;
  /** The faction that was chosen for the seat. Absent: random. */
  faction?: string;
}

/** `ti4_server::session::registry::PlayerLobbyView`: the table as everybody may see it. */
export interface LobbyState {
  game_id: string;
  phase: "lobby" | "running";
  host_player_id: string;
  slots: LobbySlot[];
  /** Counts the changes of the lobby record. Presence does not change it. */
  lobby_version: number;
  bot_kinds: string[];
  map: MapChoiceView;
  /** Changes when the board of the preview changes. */
  map_revision: number;
  strategy_card_set: string;
  speaker_order: string;
  factions: string[];
  /** The wait before each answer of a seat that the server plays, in milliseconds. */
  bot_delay_ms: number;
  /** The scenario that the game starts from. Absent: round 1. */
  start_from?: string;
}

/** `ti4_server::ws::table::TableServerMessage` */
export type TableServerMessage =
  | { type: "table"; protocol_version: number; viewer?: string; lobby: LobbyState }
  | { type: "pong"; protocol_version: number }
  | { type: "error"; protocol_version: number; kind: string; message: string };
