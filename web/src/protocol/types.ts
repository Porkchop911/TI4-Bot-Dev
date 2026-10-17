/**
 * The wire protocol that web reads and sends: the types generated from Rust
 * (`web-shared/src/wire/`, `cargo wire-web`), with their doc comments. Below them are only what web
 * narrows or keeps of its own, and the lobby of the server, which is not generated.
 */
import type {
  ChoiceOption,
  DecisionContext,
  MapChoiceView,
  ReactionMode,
  ViewerRole as WireViewerRole,
} from "@ti4/shared/wire/index.ts";

export * from "@ti4/shared/wire/index.ts";

/** Who looks at the game; a player's own connection also holds its session token. */
export type ViewerRole = WireViewerRole & { playerSession?: string };

/** The modes web offers for a reaction card; the server also knows `ownturn`. */
export type ReactionModeSetting = Exclude<ReactionMode, "ownturn">;

/** The viewing seat's own choices by printed card name; a card that is not there is "always". */
export type ReactionModes = Record<string, ReactionMode>;

/** Engine `GameState` is intentionally passed through without a presentation projection. */
export type GameState = Record<string, unknown>;

/**
 * The open question of the viewer as web keeps it: the engine's `Choice` with the nonce of its
 * `PendingChoiceEnvelope` beside it.
 */
export interface PendingChoiceDto {
  prompt: string;
  actor: string;
  nonce: string;
  options: ChoiceOption[];
  context?: DecisionContext;
  /** Display-only facts the server adds for some decisions (pools, who played a card, ...). */
  details?: Record<string, unknown>;
}

// ---- The lobby of a game on the server -------------------------------------------------------
// The types of `ti4-server` are not generated.

export type LobbyPhase = "lobby" | "running";

export interface LobbySlot {
  slot_id: string;
  position: number;
  occupant: string | null;
  nickname: string | null;
  connected: boolean;
  can_take_over: boolean;
  /**
   * `"random"` for an in-process random bot, `"smart"` for an in-process rule bot; absent for a
   * person (and for an MLP bot).
   */
  bot?: string;
  /** The faction the host chose for a bot seat; absent for random (and for every person). */
  faction?: string;
}

/** The bots a server plays itself: uniformly random, or random with simple board heuristics. */
export type InProcessBotKind = "random" | "smart";

export interface LobbyDto {
  game_id: string;
  phase: LobbyPhase;
  lobby_version: number;
  host_player_id: string;
  slots: LobbySlot[];
  /**
   * Bot kinds the host can add, all played by the server: `random`, `smart` and `mlp:<name>` for each
   * model it loaded. Absent from servers that predate random bots, which offer neither random bot.
   */
  bot_kinds?: string[];
  /** Faction ids a bot seat can be given; absent from servers that predate the choice. */
  factions?: string[];
  /** What the table plays on; absent from servers that predate the map choice. Never a seed. */
  map?: MapChoiceView;
  /** Changes whenever the previewed board changes (new choice, re-roll, new seat order). */
  map_revision?: number;
  /** The strategy card set this table plays; absent from servers that predate the option. */
  strategy_card_set?: StrategyCardSetId;
}

/** Catalog `strategy_card_sets` a game may be created with (`card_set::OFFERED` on the server). */
export type StrategyCardSetId = "te" | "pok" | "base_game_codex1";

export interface CreateGameResponse {
  game_id: string;
  player_session: string;
  player: { id: string };
  lobby: LobbyDto;
}

export interface JoinResponse {
  player_session?: string;
  player: { id: string };
  lobby: LobbyDto;
}

// Names web used before the types were generated (the Rust names, with the old Dto suffix).
export type ChoiceOptionDto = ChoiceOption;
export type MapChoiceDto = import("@ti4/shared/wire/index.ts").MapChoiceView;
export type TriggerUnitsDto = import("@ti4/shared/wire/index.ts").TriggerUnits;
export type EffectPreviewDto = import("@ti4/shared/wire/index.ts").EffectPreview;
export type ActiveEffectDto = import("@ti4/shared/wire/index.ts").ActiveEffectView;
export type EffectExpiryDto = import("@ti4/shared/wire/index.ts").EffectExpiry;
export type AgendaPeekCardDto = import("@ti4/shared/wire/index.ts").AgendaPeekCard;
export type MapPreviewDto = import("@ti4/shared/wire/index.ts").MapPreview;
