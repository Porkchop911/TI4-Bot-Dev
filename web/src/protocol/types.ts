/**
 * TypeScript definitions for the Twilight Imperium 4 Wire Protocol DTOs.
 * Exactly mirrors Rust structs from `crates/ti4-server/src/protocol/`.
 */

export const PROTOCOL_VERSION = 3;

export type ViewerRole =
  | { role: "player"; seat: string; playerSession?: string }
  | { role: "spectator" };

export type LobbyPhase = "lobby" | "running";

export interface LobbySlot {
  slot_id: string;
  position: number;
  occupant: string | null;
  nickname: string | null;
  ready: boolean;
  connected: boolean;
  can_take_over: boolean;
}

export interface LobbyDto {
  game_id: string;
  phase: LobbyPhase;
  lobby_version: number;
  host_player_id: string;
  slots: LobbySlot[];
  bot_service_enabled?: boolean;
}

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

export type PublicTurnStatus =
  | { kind: "active_turn"; player: string; phase: string; round: number }
  | { kind: "waiting_for_decision"; seat: string; phase: string; round: number; stage: string }
  | { kind: "phase_transition"; phase: string; round: number }
  | { kind: "game_over"; winner?: string | null };

export type RejectionReason =
  | { reason: "stale_version"; expected: number; current: number }
  | { reason: "stale_nonce" }
  | { reason: "unauthorized_seat"; seat?: string | null }
  | { reason: "no_pending_choice" }
  | { reason: "unknown_option"; option_id: string }
  | { reason: "validation_failed"; message: string };

export type DecisionTargetDto =
  | { System: string }
  | { Planet: { system: string; planet: string } }
  | { Unit: { system: string; unit: string } }
  | { Player: string };

export interface ChoiceOptionDto {
  id: string;
  kind?: string;
  label: string;
  description?: string;
  payload?: Record<string, unknown>;
}

export interface DecisionContextDto {
  version?: number;
  actor?: string;
  source?: Record<string, unknown>;
  subtype: string;
  phase?: string;
  round?: number;
  optional?: boolean;
  space_battle?: boolean;
  invasion_seq?: number;
  target?: DecisionTargetDto | null;
  outstanding?: OutstandingConstraintDto[];
  kind?: string;
  details?: Record<string, unknown>;
}

export interface OutstandingConstraintDto {
  kind?: string;
  amount?: number;
  paid?: number;
  min_selection?: number;
  max_selection?: number;
}

export interface PendingChoiceDto {
  prompt: string;
  actor: string;
  nonce: string;
  options: ChoiceOptionDto[];
  context?: DecisionContextDto;
}

/** Serde shape of `ti4_engine::choice::Choice` on the wire. */
export interface EngineChoice {
  player: string;
  prompt: string;
  options: ChoiceOptionDto[];
  context?: DecisionContextDto;
}

/** Opaque submission capability kept outside the engine choice contract. */
export interface PendingChoiceEnvelope {
  nonce: string;
  choice: EngineChoice;
}

/** Versioned, reconstructible static galaxy geometry. */
export interface GalaxyLayout {
  version: number;
  active_sources: string[];
  placements: Array<{ system_id: string; q: number; r: number }>;
  off_map_system_ids?: string[];
}

/** Engine `GameState` is intentionally passed through without a presentation projection. */
export type GameState = Record<string, unknown>;

export interface PlanetView {
  planet_id: string;
  controlled_by?: string | null;
  exhausted: boolean;
  attachments?: string[];
}

export interface PlacedUnitView {
  unit_type: string;
  owner: string;
  planet?: string | null;
  damaged: boolean;
}

export interface SystemView {
  system_id: string;
  command_tokens: string[];
  planets: Record<string, PlanetView>;
  units: PlacedUnitView[];
  coordinate?: string;
  tile_type?: string;
}

export interface PlayerView {
  id: string;
  faction: string;
  victory_points: number;
  trade_goods: number;
  commodities: number;
  tactic_tokens: number;
  fleet_tokens: number;
  strategic_tokens: number;
  passed: boolean;
  strategy_cards: string[];
  exhausted_strategy_cards: string[];
  technologies: string[];
  exhausted_technologies: string[];
  relics: string[];
  exhausted_relics: string[];
  action_cards_count: number;
  secret_objectives_count: number;
  held_action_cards?: string[];
  held_secret_objectives?: string[];
  scored_secret_objectives?: string[];
  leaders: Record<string, string>;
}

export interface ObjectiveProgressView {
  have: number;
  threshold: number;
  satisfied: boolean;
}

export interface TableView {
  revealed_objectives: string[];
  scored_objectives: Record<string, string[]>;
  objective_progress?: Record<string, Record<string, ObjectiveProgressView>>;
  unclaimed_strategy_cards: string[];
  strategy_card_goods: Record<string, number>;
  laws: Record<string, string>;
}

export interface PlanetMetaView {
  id: string;
  label: string;
  resources: number;
  influence: number;
  traits?: string[];
  tech_specialties?: string[];
  legendary?: boolean;
  space_station?: boolean;
}

export interface BoardTileView {
  system_id: string;
  label: string;
  q: number;
  r: number;
  hyperlane?: boolean;
  special_area?: string | null;
  anomalies?: string[];
  wormholes?: string[];
  egress?: boolean;
  planets?: PlanetMetaView[];
}

export interface CombatDieRoll {
  player?: string | null;
  unit: string;
  roll: number;
  target: number;
  hit: boolean;
}

export interface CombatView {
  system_id: string;
  round: number;
  battle_seq?: number;
  phase?: "pre_roll" | "barrage" | "resolving_hits" | "retreating" | "complete";
  round_start?: PlacedUnitView[];
  barrage_start?: PlacedUnitView[];
  barrage_hits?: Record<string, number>;
  barrage_dice?: CombatDieRoll[];
  remaining_hits?: Record<string, number>;
  attacker: string;
  defender: string;
  active_player?: string | null;
  stage?: string | null;
  hits_to_assign?: number | null;
  attacker_hits?: number | null;
  defender_hits?: number | null;
  dice_rolls?: CombatDieRoll[];
}

export interface BoardView {
  systems: Record<string, SystemView>;
  active_system?: string | null;
  map_tiles?: BoardTileView[];
  combat?: CombatView | null;
  invasion?: InvasionView | null;
}

export interface InvasionView {
  system_id: string;
  invasion_seq: number;
  invader: string;
  phase: string;
  planets: string[];
  current_planet: string | null;
  defender: string | null;
  ground_round: number;
  last_step?: InvasionStepView | null;
  odds_context?: Record<string, InvasionOddsContext>;
}

export interface InvasionOddsContext {
  opponent: string | null;
  available: boolean;
  ground_force_types: string[];
  additional_guns: Record<string, number>;
  harrow_units: Record<string, number>;
}

export interface InvasionStepView {
  planet: string;
  kind: string;
  round: number;
  before: PlacedUnitView[];
  after: PlacedUnitView[];
  dice: {
    planet: string;
    player: string;
    group: string;
    face: number;
    target: number;
    hit: boolean;
  }[];
  hits: Record<string, number>;
  harrow_hits: number;
}

export interface GameView {
  round: number;
  phase: string;
  speaker: string;
  seating_order: string[];
  active_player?: string | null;
  finished: boolean;
  players: PlayerView[];
  board: BoardView;
  table: TableView;
}

export type EventVisibility =
  | { visibility: "public" }
  | { visibility: "seat"; seat: string }
  | { visibility: "referee" };

export type GameEventKind =
  | { kind: "game_initialized"; round: number; phase: string; speaker: string }
  | { kind: "decision_resolved" }
  | { kind: "phase_transition"; phase: string; round: number }
  | { kind: "game_finished"; winner?: string | null };

export type GameEvent = EventVisibility & {
  id: string;
  timestamp: string;
  version?: number;
  decision_count?: number;
  batch_id?: string;
  batch_start_cursor?: number;
  batch_end_cursor?: number;
  action_id?: string;
  action_start_cursor?: number;
  actor?: string;
  round?: number;
  phase?: string;
  action_type?: string;
  action_actor?: string;
  stage?: string;
  detail?: string;
  private_detail?: string;
  movement?: { actor: string; origin: string; destination: string; unit: string };
  event: GameEventKind;
};

export interface HistoryStatus {
  cursor: number;
  redo_count: number;
  generation?: number;
}

export interface CurrentLogPath {
  round: number;
  phase: string;
  action_id?: string;
  stage?: string;
}

export interface GameEventMsg {
  type?: "event";
  protocol_version: number;
  game_id: string;
  entry: GameEvent;
}

export interface InitialSnapshotMsg {
  type?: "initial_snapshot";
  protocol_version: number;
  game_id: string;
  game_version: number;
  viewer: ViewerRole;
  view: GameView;
  state: GameState;
  galaxy_layout: GalaxyLayout;
  pending_choice?: PendingChoiceEnvelope | null;
  turn_status: PublicTurnStatus;
  events?: GameEvent[];
  history?: HistoryStatus;
  current_path?: CurrentLogPath;
}

export interface StateUpdateMsg {
  type?: "state_update";
  protocol_version: number;
  game_id: string;
  game_version: number;
  viewer: ViewerRole;
  view: GameView;
  state: GameState;
  galaxy_layout: GalaxyLayout;
  pending_choice?: PendingChoiceEnvelope | null;
  turn_status: PublicTurnStatus;
  history?: HistoryStatus;
  current_path?: CurrentLogPath;
}

export interface PendingChoiceMsg {
  type?: "pending_choice";
  protocol_version: number;
  game_id: string;
  game_version: number;
  nonce: string;
  choice: EngineChoice;
  state: GameState;
  galaxy_layout: GalaxyLayout;
}

export interface TurnStatusMsg {
  type?: "turn_status";
  protocol_version: number;
  game_id: string;
  game_version: number;
  status: PublicTurnStatus;
}

export interface ActionAcceptedMsg {
  type?: "action_accepted";
  protocol_version: number;
  game_id: string;
  game_version: number;
  option_id: string;
}

export interface ActionRejectedMsg {
  type?: "action_rejected";
  protocol_version: number;
  game_id: string;
  game_version: number;
  reason: RejectionReason;
}

export interface ProtocolErrorMsg {
  type?: "error";
  protocol_version: number;
  kind: string;
  message: string;
}

export interface GameOverMsg {
  type?: "game_over";
  protocol_version: number;
  game_id: string;
  game_version: number;
  winner?: string | null;
  final_scores: Record<string, number>;
}

export interface PongMsg {
  type?: "pong";
  protocol_version: number;
  sequence: number;
}

export type ServerMessage =
  | ({ type: "initial_snapshot" } & InitialSnapshotMsg)
  | ({ type: "state_update" } & StateUpdateMsg)
  | ({ type: "pending_choice" } & PendingChoiceMsg)
  | ({ type: "turn_status" } & TurnStatusMsg)
  | ({ type: "action_accepted" } & ActionAcceptedMsg)
  | ({ type: "action_rejected" } & ActionRejectedMsg)
  | ({ type: "error" } & ProtocolErrorMsg)
  | ({ type: "game_over" } & GameOverMsg)
  | ({ type: "pong" } & PongMsg)
  | ({ type: "event" } & GameEventMsg);

export type ClientMessage =
  | {
      type: "subscribe";
      protocol_version: number;
      game_id: string;
      player_session?: string;
    }
  | {
      type: "submit_choice";
      protocol_version: number;
      game_id: string;
      nonce: string;
      expected_version: number;
      option_id: string;
    }
  | {
      type: "ping";
      protocol_version: number;
      sequence: number;
    };
