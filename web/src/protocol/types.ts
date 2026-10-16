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
  bot_service_enabled?: boolean;
  /**
   * Bot kinds the host can add: `random` and `smart` (in-process, no password) and `mlp` (needs the bot
   * password). Absent from servers that predate random bots, which offer neither random bot.
   */
  bot_kinds?: string[];
  /** Faction ids a bot seat can be given; absent from servers that predate the choice. */
  factions?: string[];
  /** What the table plays on; absent from servers that predate the map choice. Never a seed. */
  map?: MapChoiceDto;
  /** Changes whenever the previewed board changes (new choice, re-roll, new seat order). */
  map_revision?: number;
  /** The strategy card set this table plays; absent from servers that predate the option. */
  strategy_card_set?: StrategyCardSetId;
}

/** Catalog `strategy_card_sets` a game may be created with (`card_set::OFFERED` on the server). */
export type StrategyCardSetId = "te" | "pok" | "base_game_codex1";

/** The host's choice: a predefined layout or the seeded random board. */
export type MapChoice = { kind: "template"; alias: string } | { kind: "random" };

export interface MapChoiceDto {
  kind: "template" | "random";
  alias?: string;
  /** Display name of a template that has one (the community layouts). */
  name?: string;
  author?: string;
  systems: number;
  hyperlanes: boolean;
  recommended: boolean;
}

export interface MapTemplateSummary {
  alias: string;
  /** Display name of a template that has one (the community layouts); else titled from the alias. */
  name?: string;
  author: string;
  player_count: number;
  buildable: boolean;
  systems: number;
  hyperlanes: boolean;
  recommended: boolean;
}

export interface MapSeatPreview {
  /** 1-based number of the place. Who sits there is drawn when the game starts. */
  seat: number;
  /** The id of the preview tile that stands for the home of this seat (`seat_1`, …). */
  home_system_id: string;
}

export interface MapPreviewDto {
  choice: MapChoice;
  player_count: number;
  tiles: BoardTileView[];
  seats: MapSeatPreview[];
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
  /** A reaction window: names no seat, because who holds a playable card is private. */
  | { kind: "waiting_for_reactions"; phase: string; round: number }
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
  auto_resolved?: boolean;
}

/** What happened to open a reaction window: public facts only (engine `DecisionTrigger`). */
export type TriggerKindDto =
  | "action_card_played"
  | "action_card_discarded"
  | "system_activated"
  | "ship_moved"
  | "strategic_action_began"
  | "strategy_card_chosen"
  | "strategy_phase_began"
  | "turn_began"
  | "turn_passed"
  | "player_passed"
  | "action_completed"
  | "strategy_cards_would_return"
  | "agenda_phase_began"
  | "agenda_revealed"
  | "votes_cast"
  | "agenda_resolved"
  | "transaction"
  | "planet_control_gained"
  | "invasion_began"
  | "units_committed"
  | "ground_rolls"
  | "combat_started"
  | "anti_fighter_barrage"
  | "space_cannon_hits"
  | "hits_to_assign"
  | "sustain_damage"
  | "ship_destroyed"
  | "retreat"
  | "space_combat_won"
  | "production_used"
  | "unit_ability_rolled"
  | "other";

export interface TriggerUnitsDto {
  owner: string;
  unit_type: string;
  count: number;
}

export interface DecisionTriggerDto {
  kind: TriggerKindDto;
  event_type: string;
  event_id: number;
  relation: "when" | "after";
  actor?: string;
  subject?: string;
  card?: string;
  agenda?: string;
  system?: string;
  planet?: string;
  units?: TriggerUnitsDto[];
  hits?: number;
  chain?: number[];
}

export type HitCauseDto =
  | "assault_cannon"
  | "courageous_to_the_end"
  | "anti_fighter_barrage"
  | "space_cannon"
  | "combat_roll"
  | "start_of_combat"
  | "end_of_round"
  | "other";

/** What kind of hit a casualty decision assigns (display only; absent from older servers). */
export interface HitDetailDto {
  cause: HitCauseDto;
  /** A destroy effect: not a hit, so Sustain Damage does not apply. */
  destroy: boolean;
  restriction: "any" | "non_fighter";
  producer?: string;
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
  /** Present on reaction decisions from servers that send it; absent in older saves and fixtures. */
  trigger?: DecisionTriggerDto | null;
  hit?: HitDetailDto | null;
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
  /** Display-only facts the server adds for some decisions (pools, who played a card, ...). */
  details?: Record<string, unknown>;
}

/** Serde shape of `ti4_engine::choice::Choice` on the wire. */
export interface EngineChoice {
  player: string;
  prompt: string;
  options: ChoiceOptionDto[];
  context?: DecisionContextDto;
  details?: Record<string, unknown>;
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
  /** Progress toward this seat's own countable secrets; only ever sent to that seat. */
  secret_objective_progress?: Record<string, ObjectiveProgressView>;
  leaders: Record<string, string>;
  /** Other players' command tokens in this seat's fleet pool (Mahact), one entry per token. */
  held_foreign_tokens?: string[];
}

export interface ObjectiveProgressView {
  have: number;
  threshold: number;
  satisfied: boolean;
  /** Push Boundaries: this player's planet count against each neighbour's (public). */
  neighbours?: NeighbourProgressView[];
  /** Learn the Secrets of the Cosmos: the systems that count right now (holder only). */
  systems?: string[];
}

export interface NeighbourProgressView {
  player: string;
  /** Planets the progress owner controls. */
  mine: number;
  /** Planets this neighbour controls. */
  theirs: number;
  /** Strictly more planets than this neighbour: the row counts toward the objective. */
  ahead: boolean;
}

export interface TableView {
  revealed_objectives: string[];
  scored_objectives: Record<string, string[]>;
  objective_progress?: Record<string, Record<string, ObjectiveProgressView>>;
  /** Rule 61.16: per seat barred from scoring public objectives, the home planets it does not control. */
  home_system_lost?: Record<string, string[]>;
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
  | { kind: "dice_rolled" }
  | { kind: "phase_transition"; phase: string; round: number }
  | { kind: "game_finished"; winner?: string | null };

/** One public die roll (a `dice_rolled` event): who rolled, what for, what showed, what it did. */
export interface PublicRoll {
  /** Position in the game's roll history; a replay of the same roll has the same index. */
  index: number;
  roller?: string;
  /** The engine's tag ("gravity rift", "space combat"). */
  reason: string;
  /** The tag in words ("Gravity rift"). */
  label: string;
  faces: number[];
  hits_on?: number;
  hits?: number;
  /** Positions of the dice that are rerolls. */
  rerolled?: number[];
  /** The rift exited, else the system in play. */
  system?: string;
  /** "ship destroyed", "2 hits". */
  outcome?: string;
}

export type GameEvent = {
  /** Nested object on the wire; decode with `visibilityKind` (protocol/eventVisibility.ts). */
  visibility: EventVisibility;
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
  /** Present on `dice_rolled` events. */
  roll?: PublicRoll;
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
  reaction_modes?: ReactionModes;
}

/** How a seat wants an action card handled in reaction windows. Absent means always offered. */
export type ReactionModeSetting = "always" | "never";

/** The viewing seat's own choices by printed card name; the server only lists "never". */
export type ReactionModes = Record<string, ReactionModeSetting>;

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
  /** Decisions the engine made for this seat since the last update because only one option was legal. */
  auto_resolved?: AutoResolvedNote[];
  reaction_modes?: ReactionModes;
}

/** One decision settled on the viewer's behalf (single legal option). Feedback only. */
export interface AutoResolvedNote {
  id: string;
  /** The question that was not asked. */
  prompt: string;
  /** What was chosen, as labelled. */
  selected: string;
  /** Why there was no real choice. */
  reason: string;
  /** Identical notes this one stands for; absent means one. */
  count?: number;
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

/**
 * The seat's own bluff settings. The server sends this only to the declaring seat (never to other
 * seats or spectators) and it carries no game version, so it is never dropped as stale.
 */
export interface ReactionIntentStateMsg {
  type?: "reaction_intent_state";
  protocol_version: number;
  game_id: string;
  /** Declared trigger ids (see `bluffTriggers.ts`). */
  triggers: string[];
  max_triggers: number;
  /** Whether this seat may bluff now: it holds action cards and set no card to Never offer. */
  eligible: boolean;
  ineligible_reason?: string;
  /** The declaration cannot change before this round starts. */
  locked_until_round?: number;
  /** A stall budget is used up: no more holds. Private to this seat. */
  budget_used_up: boolean;
  /** A hold for this seat is running now. */
  holding: boolean;
}

/** Why the server refused a secondary preview (`ti4_server::protocol::server::PreviewRefusal`). */
export type PreviewRefusal =
  | "not_seated"
  | "no_strategic_action"
  | "is_primary"
  | "already_asked"
  | "busy"
  | "rate_limited";

/** Why a follower would not be asked a secondary at all (`ti4_engine::strategy::SecondaryBlocker`). */
export type SecondaryBlocker =
  | "no_strategy_token"
  | "cannot_afford_influence"
  | "cannot_pay_resources"
  | "nothing_to_research"
  | "no_exhausted_planet"
  | "commodities_full"
  | "no_production"
  | "other";

/** How the engine would pay a Technology secondary's resources. */
export interface PreviewPayment {
  cost: number;
  planets: string[];
  trade_goods: number;
  worth: number;
}

/** What the engine says the seat would be asked (`ti4_engine::secondary_preview::SecondaryPreview`). */
export type SecondaryPreviewBody =
  | {
      status: "question";
      choice: EngineChoice;
      step: number;
      payment?: PreviewPayment;
      skipped?: { subtype: string; prompt: string }[];
    }
  | { status: "would_not_be_asked"; blocker: SecondaryBlocker }
  | {
      status: "complete";
      skipped?: { subtype: string; prompt: string }[];
      /** Scripted answers after the first that the engine never asked for. */
      unused_answers?: number;
    }
  /** A scripted answer is not offered by the question the engine asks at that point. */
  | { status: "rejected"; at: number; answer: string; choice: EngineChoice }
  | { status: "unavailable"; detail: string };

/**
 * The answer to a `preview_secondary` request: what the asking seat would be asked for the
 * secondary in progress if its window opened now. Private to the asking connection, ephemeral,
 * and without a game version of its own (so it is never dropped as stale): `as_of_version` says
 * which position it was computed at.
 */
export interface SecondaryPreviewMsg {
  type?: "secondary_preview";
  protocol_version: number;
  game_id: string;
  request_id: number;
  as_of_version: number;
  as_of_decisions: number;
  card: string;
  outcome:
    | { result: "preview"; preview: SecondaryPreviewBody }
    | { result: "refused"; reason: PreviewRefusal; detail: string };
}

/** One kind of ship in one origin system that could move into the queried system. */
export interface ReachShip {
  type_id: string;
  count: number;
  damaged: number;
  /** Of `count`, how many arrive only by spending Gravity Drive. */
  gravity_drive: number;
  /** Of `count`, how many arrive only by exhausting the Ionian Fuel Refinery. */
  ionian: number;
}

export interface ReachOrigin {
  owner: string;
  origin: string;
  ships: ReachShip[];
}

export type ReachRefusal = "busy" | "unknown_system" | "rate_limited";

/**
 * The answer to a `query_reach` request ("who can go here"): the ships that could move into
 * `system_id` if it were activated now, from the engine's own movement search. Private to the asking
 * connection and ephemeral; `as_of_version` says which position it was computed at.
 */
export interface ReachIntoMsg {
  type?: "reach_into";
  protocol_version: number;
  game_id: string;
  request_id: number;
  system_id: string;
  as_of_version: number;
  origins: ReachOrigin[];
  refused?: ReachRefusal;
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
  | ({ type: "event" } & GameEventMsg)
  | ({ type: "reaction_intent_state" } & ReactionIntentStateMsg)
  | ({ type: "secondary_preview" } & SecondaryPreviewMsg)
  | ({ type: "reach_into" } & ReachIntoMsg);

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
      type: "set_reaction_mode";
      protocol_version: number;
      game_id: string;
      /** The printed card name; every copy of it is covered. */
      card: string;
      mode: ReactionModeSetting;
    }
  | {
      type: "set_reaction_intent";
      protocol_version: number;
      game_id: string;
      /** Trigger ids this seat bluffs about; the whole declaration, empty clears it. */
      triggers: string[];
    }
  | {
      type: "pass_reaction_hold";
      protocol_version: number;
      game_id: string;
    }
  | {
      /** Read-only: what would my own seat be asked for this secondary if its window opened now? */
      type: "preview_secondary";
      protocol_version: number;
      game_id: string;
      request_id: number;
      card: string;
      primary: string;
      answers: string[];
    }
  | {
      /** Read-only: which ships could move into this system if it were activated now? */
      type: "query_reach";
      protocol_version: number;
      game_id: string;
      request_id: number;
      system_id: string;
    }
  | {
      type: "ping";
      protocol_version: number;
      sequence: number;
    };
