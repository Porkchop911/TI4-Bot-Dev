/**
 * The part of the wire protocol that web2 reads. The shapes are the serde output of the Rust
 * types named in each comment (`crates/ti4-view`, `crates/ti4-engine`). The server sends them in
 * `StateUpdateMsg`, the wasm build hands them over as `SessionUpdate`: same names, same shapes.
 * The messages of the websocket are at the end.
 */

/** `ti4_view::status::ViewerRole` */
export type ViewerRole = { role: "player"; seat: string } | { role: "spectator" };

/** `ti4_view::status::PublicTurnStatus` */
export type PublicTurnStatus =
  | { kind: "active_turn"; player: string; phase: string; round: number }
  | { kind: "waiting_for_decision"; seat: string; phase: string; round: number; stage: string }
  /** A reaction window is open, or looks open. It names no seat: who can react is private. */
  | { kind: "waiting_for_reactions"; phase: string; round: number }
  | { kind: "phase_transition"; phase: string; round: number }
  | { kind: "game_over"; winner?: string | null };

/** `ti4_engine::choice::ChoiceOption` */
export interface ChoiceOption {
  id: string;
  kind: string;
  label: string;
  payload: Record<string, unknown>;
  auto_resolved?: boolean;
}

/** `ti4_engine::decision_context::DecisionContext`: the fields that web2 reads. */
export interface DecisionContext {
  /** What is asked: "activate_system", "movement_step", "load_cargo", "reaction_…". */
  subtype: string;
  actor: string;
}

/** `ti4_engine::choice::Choice`. `details` is not read yet. */
export interface Choice {
  player: string;
  prompt: string;
  options: ChoiceOption[];
  context?: DecisionContext | null;
  details?: Record<string, unknown>;
}

/** `ti4_view::projection::PendingChoiceEnvelope` */
export interface PendingChoiceEnvelope {
  nonce: string;
  choice: Choice;
}

/** `ti4_view::view::PlayerView` */
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
  /** The legendary planets whose ability card is exhausted. */
  exhausted_legendary?: string[];
  /** Relic fragments by deck: CULTURAL, HAZARDOUS, INDUSTRIAL, FRONTIER. */
  relic_fragments?: Record<string, number>;
  exploration_cards?: string[];
  action_cards_count: number;
  secret_objectives_count: number;
  held_action_cards?: string[];
  held_secret_objectives?: string[];
  scored_secret_objectives?: string[];
  leaders: Record<string, string>;
}

/** `ti4_view::view::PlanetView` */
export interface PlanetView {
  planet_id: string;
  controlled_by: string | null;
  exhausted: boolean;
  attachments?: string[];
}

/** `ti4_view::view::PlacedUnitView` */
export interface PlacedUnitView {
  unit_type: string;
  owner: string;
  planet: string | null;
  damaged: boolean;
  galvanized?: boolean;
}

/** `ti4_view::view::SystemView` */
export interface SystemView {
  system_id: string;
  command_tokens: string[];
  planets: Record<string, PlanetView>;
  units: PlacedUnitView[];
}

/** `ti4_view::view::PlanetMetaView` */
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

/** `ti4_view::view::BoardTileView` */
export interface BoardTileView {
  system_id: string;
  label: string;
  q: number;
  r: number;
  hyperlane?: boolean;
  special_area?: string;
  anomalies?: string[];
  wormholes?: string[];
  egress?: boolean;
  planets?: PlanetMetaView[];
}

/** `ti4_view::view::BoardView`. `combat` and `invasion` are not read yet. */
export interface BoardView {
  systems: Record<string, SystemView>;
  active_system?: string;
  map_tiles?: BoardTileView[];
  combat?: unknown;
  invasion?: unknown;
}

/** `ti4_view::view::TableView`. `objective_progress` is not read yet. */
export interface TableView {
  revealed_objectives: string[];
  scored_objectives: Record<string, string[]>;
  objective_progress?: unknown;
  unclaimed_strategy_cards: string[];
  strategy_card_goods: Record<string, number>;
  laws: Record<string, string>;
}

/** `ti4_view::view::GameView` */
export interface GameView {
  round: number;
  /** The turn of the action phase, from 1; 0 in the other phases. */
  turn: number;
  phase: string;
  speaker: string;
  seating_order: string[];
  active_player?: string;
  finished: boolean;
  players: PlayerView[];
  board: BoardView;
  table: TableView;
}

/** `ti4_view::tactical::MoveFact` */
export interface MoveFact {
  /** The ship cannot arrive without Gravity Drive. */
  gravity_drive: boolean;
  ionian: boolean;
  /** The systems of the way, from the system of the ship to the active system. */
  path: string[];
  /** The gravity rifts that the ship leaves: one roll for each. */
  rifts: string[];
}

/** `ti4_view::tactical::ShipFact` */
export interface ShipFact {
  origin: string;
  index: number;
  unit: string;
  damaged: boolean;
  capacity: number;
  /** The ship counts against the fleet pool. */
  fleet: boolean;
  /** The move value in this activation, without Gravity Drive and the Ionian Fuel Refinery. */
  move_value: number;
  /** The ship starts in a nebula: it moves with a value of 1. */
  nebula?: boolean;
  move?: MoveFact;
  blocked?: "command_token" | "range";
  /** What the ship can load: places in `MovementFacts.cargo`. */
  loads?: number[];
}

/** `ti4_view::tactical::CargoPool` */
export interface CargoPool {
  system: string;
  /** The planet, or null for the space area. */
  source: string | null;
  unit: string;
  damaged: boolean;
  galvanized: boolean;
  count: number;
}

/** `ti4_view::tactical::ActivationFacts` */
export interface ActivationFacts {
  kind: "activation";
  tactic_tokens: number;
  systems: {
    system: string;
    ships: number;
    origins: number;
    /** The production value of the seat in the system, when it is more than 0. */
    production?: number;
    /** The guns of the seat that roll SPACE CANNON at the activation, when there are any. */
    cannon?: number;
  }[];
}

/** `ti4_view::tactical::MovementFacts` */
export interface MovementFacts {
  kind: "movement";
  active: string;
  ships: ShipFact[];
  cargo: CargoPool[];
  /** Gravity Drive can still give one ship of this action its move. */
  gravity_drive: boolean;
  ionian: boolean;
  fleet_supply: { limit: number; charged: number };
  /** How many ships already moved in this activation. */
  moved: number;
  /** The space combat that ships moving in start: only when another seat has ships in the active system. */
  battle?: BattleFacts;
}

/**
 * `ti4_view::tactical::CombatSide`: one side of a space combat as the state has it, by unit id.
 * It is a side of `ti4_space_odds` as it is.
 */
export interface CombatSide {
  faction: string;
  /** The ships of the side in the active system. */
  units?: Record<string, number>;
  /** How many of them are damaged. */
  damaged?: Record<string, number>;
  /** The units of the side that fire SPACE CANNON in the active system before the combat. */
  guns?: Record<string, number>;
}

/** `ti4_view::tactical::BattleFacts`: the space combat in the active system, before a ship moves in. */
export interface BattleFacts {
  /** The seat that defends. */
  defender: string;
  /** The seat that moves, with what it has there already. The page adds the ships that it moves in. */
  attacker: CombatSide;
  defender_side: CombatSide;
}

/** What `ti4_space_odds` reads: two sides of a space combat. */
export interface SpaceOddsRequest {
  attacker: CombatSide;
  defender: CombatSide;
}

/** `ti4_calc::odds::BattleResponse`: the rates are fractions of the rollouts, the survivors by unit id. */
export interface SpaceOddsResponse {
  simulations: number;
  attacker_win_rate: number;
  defender_win_rate: number;
  mutual_destruction_rate: number;
  /** The rollouts that did not end. */
  unresolved_rate: number;
  average_rounds: number;
  attacker_expected_survivors: Record<string, number>;
  defender_expected_survivors: Record<string, number>;
  attacker_fielded: Record<string, number>;
  defender_fielded: Record<string, number>;
}

/** `ti4_view::tactical::LandingPool` */
export interface LandingPool {
  unit: string;
  damaged: boolean;
  /** Where the units are, when it is not the space area of the active system. */
  from_system?: string;
  from_planet?: string;
  count: number;
}

/** `ti4_view::tactical::InvasionFacts` */
export interface InvasionFacts {
  kind: "invasion";
  active: string;
  forces: LandingPool[];
  /** Every planet of the active system. `lands`: what can land there, places in `forces`. */
  planets: { planet: string; lands?: number[] }[];
}

/** `ti4_view::tactical::BuildFact`: one kind of unit of a production, also one that the seat cannot produce now. */
export interface BuildFact {
  unit: string;
  /** The printed cost of one purchase, before a discount. */
  cost: number;
  /** How many units one purchase makes: 2 for fighters and infantry. */
  per: number;
  /** How many are left in the reinforcements. Absent when there is no limit. */
  supply?: number;
  /** The unit is a ship that counts against the fleet pool. */
  fleet?: boolean;
  /** The unit uses capacity while it is in a space area. */
  cargo?: boolean;
  /** Where the unit can be placed: "space", or a planet of the system. */
  places: string[];
  /** What the unit hits on in a combat, and with how many dice. Absent: it does not fight. */
  combat?: { hits_on: number; dice: number };
  /** The move value. 0: the unit does not move by itself. */
  move_value: number;
  /** What the unit carries. */
  capacity: number;
  /** The keyword abilities as printed: "SUSTAIN DAMAGE", "BOMBARDMENT 5+", "PRODUCTION +2". */
  abilities?: string[];
  /** The special rule that is printed on the unit. Absent: it has none. */
  rules?: string;
  /** Why the seat cannot produce the unit here now. Absent: it can. */
  unavailable?: "technology" | "supply" | "blockade" | "no_place" | "not_offered";
}

/** `ti4_view::tactical::ProductionFacts` */
export interface ProductionFacts {
  kind: "production";
  /** The system that produces. */
  system: string;
  /** The production limit of this use, and the units that it already made. */
  capacity: { limit: number; used: number };
  units: BuildFact[];
  /** What the combined cost of this use is still reduced by. */
  discount: number;
  /** Resources that are paid and not used yet. */
  credit: number;
  free?: boolean;
  /** The ready planets of the seat that pay resources, with what each is worth. */
  planets: { planet: string; worth: number }[];
  trade_goods: number;
  trade_good_worth: number;
  fleet_supply: { limit: number; charged: number };
  /** The capacity of the ships of the seat in the system, and what uses it. */
  transport: { limit: number; used: number };
}

/** `ti4_model::state::ExplorationRecord`: one card that was drawn. */
export interface ExplorationRecord {
  round: number;
  player: string;
  deck: string;
  card: string;
  /** Null for a frontier token. */
  planet: string | null;
  /** `fragment`, `attached:<attachment>`, `resolved`, `unresolved` or `discarded`. */
  outcome: string;
}

/** `ti4_view::exploration::ExplorationFacts` */
export interface ExplorationFacts {
  /** The card ids that each deck still holds, sorted. The keys are CULTURAL, HAZARDOUS, ... */
  decks: Record<string, string[]>;
  drawn: ExplorationRecord[];
  /** The planets that are explored when a player takes them, with the decks. */
  explores: Record<string, string[]>;
}

/** `ti4_view::exploration::ExplorationCatalog`: the printed exploration cards. */
export interface ExplorationCatalog {
  cards: { id: string; name: string; deck: string; resolution: string; text: string }[];
  /** The cards that each deck begins a game with. */
  decks: Record<string, string[]>;
  attachments: {
    id: string;
    name: string;
    resources: number;
    influence: number;
    tech_specialties?: string[];
    text?: string;
  }[];
}

/** `ti4_view::components::PrintedCard` */
export interface PrintedCard {
  id: string;
  name: string;
  text: string;
  /** The card has an ACTION or exhausts. A card that always holds is not used. */
  used: boolean;
}

/** `ti4_view::components::FactionSheet`: a faction as its sheet prints it. */
export interface FactionSheet {
  name: string;
  title: string;
  commodities: number;
  /** The agent, the commander and the hero, by id. */
  leaders?: string[];
  starting_technologies?: string[];
  /** A faction that chooses its starting technologies: how many, and of which. */
  starting_technology_choice?: { amount: number; options: string[] };
  faction_technologies?: string[];
  /** The units that the faction begins with. Without `planet` they are in the space area. */
  starting_fleet?: { unit: string; count: number; planet?: string }[];
  home_planets?: { id: string; name: string; resources: number; influence: number }[];
}

/** `ti4_view::components::ComponentCatalog`: the printed cards of the play areas. */
export interface ComponentCatalog {
  abilities: { id: string; name: string; window?: string; text: string }[];
  /** The ability ids of each faction, in the order of the faction sheet. */
  factions: Record<string, string[]>;
  /**
   * `ti4_view::components::FactionSheet`: the name that a table calls each faction by ("Sol"),
   * the name as printed and what the sheet of the faction says. Absent from the demo.
   */
  faction_sheets?: Record<string, FactionSheet>;
  /**
   * `ti4_view::components::StrategyCard`: the strategy cards of every set. The id of a card of
   * an older printing has no name in it (`base4`). Absent from the demo.
   */
  strategy_cards?: { id: string; name: string; initiative: number }[];
  leaders: {
    id: string;
    /** "agent", "commander" or "hero". */
    kind: string;
    name: string;
    title: string;
    window?: string;
    text: string;
    /** How the leader is unlocked. Absent for a leader that is never locked. */
    unlock?: string;
  }[];
  technologies: PrintedCard[];
  relics: PrintedCard[];
  /** The ability cards of the legendary planets, by the id of the planet. */
  legendary: PrintedCard[];
  /** The action card deck as a game begins with it. Copies have ids of their own and one name. */
  action_cards: {
    id: string;
    name: string;
    /** "Action", "Agenda", "Strategy", "Status" or "Any". */
    phase: string;
    /** When the card is played, as printed: "Action", or a timing. */
    window: string;
    /** The group of the timing, one of `action_card_groups`. */
    group: string;
    text: string;
  }[];
  /** The groups of the timings of the action cards, in the order in which they are shown. */
  action_card_groups: string[];
  /** The secret objective deck as a game begins with it. */
  secret_objectives: {
    id: string;
    name: string;
    /** The phase in which the objective is scored: "Status", "Action" or "Agenda". */
    phase: string;
    /** What the objective asks for, one of `secret_objective_groups`. */
    group: string;
    text: string;
  }[];
  /**
   * Every public objective that a game can reveal: stage I, then stage II. A game draws five of
   * each stage. Absent from an engine that is older than the fact.
   */
  public_objectives?: { id: string; name: string; points: number; group: string; text: string }[];
  /** The groups of what the public objectives ask for, in the order in which they are shown. */
  public_objective_groups?: string[];
  /** The groups of what the secret objectives ask for, in the order in which they are shown. */
  secret_objective_groups: string[];
  /** The agenda deck as a game begins with it. Every agenda is in it once. */
  agendas?: {
    id: string;
    name: string;
    /** "Law" or "Directive". */
    kind: string;
    /** What the vote is about: "For/Against", "Elect Player". Absent for Covert Legislation. */
    elect?: string;
    /** The text of the outcome "For", or of the election. */
    text: string;
    /** The text of the outcome "Against". Absent for an election. */
    against?: string;
    /** What the agenda affects, one of `agenda_groups`. */
    group: string;
  }[];
  /** The groups of what the agendas affect, in the order in which they are shown. */
  agenda_groups?: string[];
}

/** `ti4_view::score::PointSource`: a source of points that is not a public objective. */
export interface PointSource {
  kind:
    | "secrets"
    | "custodians"
    | "imperial"
    | "support"
    | "agenda"
    | "relic"
    | "action_card"
    | "planet"
    | "other";
  /** The printed name of the agenda, relic, card or planet. Empty for the first four kinds. */
  name: string;
  /** The printed text of the agenda, relic, card or planet. Absent for the first four kinds. */
  text?: string;
  /** The points of each seat, gains and losses together. A seat with none is absent. */
  points: Record<string, number>;
  /** For `support`: whose notes the seat has. */
  from?: Record<string, string[]>;
}

/** `ti4_view::score::ScoreFacts` */
export interface ScoreFacts {
  /** The points that win the game. */
  target: number;
  /** The public objectives as printed, in the order of the reveal, with the seats that scored. */
  public: { id: string; name: string; points: number; text: string; scored: string[] }[];
  /** Secrets, custodians, Imperial and Support are always there; the rest when a seat has a point. */
  sources: PointSource[];
  /** The points of a seat that no row explains. Absent in a game that the engine played. */
  unexplained?: Record<string, number>;
}

/** `ti4_view::secret_objectives::SecretObjectiveFacts` */
export interface SecretObjectiveFacts {
  /** How many cards the deck still holds. */
  deck: number;
  /** The ids of the secret objectives that each seat has scored. A seat with none is absent. */
  scored: Record<string, string[]>;
  /** How many secret objectives a seat may have, scored and unscored together. */
  hand_limit: number;
}

/** `ti4_view::agendas::AgendaFacts` */
export interface AgendaFacts {
  /** How many cards the deck still holds. */
  deck: number;
}

/** `ti4_view::action_cards::ActionCardFacts` */
export interface ActionCardFacts {
  /** How many cards the deck still holds. */
  deck: number;
  /** The ids of the discard pile. */
  discard: string[];
  /** The cards face up on the card of Garbozia. */
  salvage?: string[];
  /** Of the viewer. Absent for a spectator and for a seat with no limit. */
  hand_limit?: number;
}

/** `ti4_view::components::Catalog`: everything printed. The host asks for it once. */
export interface Catalog {
  /**
   * The digest of the corpus that the texts are of. A game on the server is played only when
   * the server has the same (`InitialSnapshotMsg.content`). Absent from an engine that is older
   * than the fact, and from the demo.
   */
  content?: string;
  exploration: ExplorationCatalog;
  components: ComponentCatalog;
}

/** `ti4_view::tactical::TacticalFacts` */
export type TacticalFacts = ActivationFacts | MovementFacts | InvasionFacts | ProductionFacts;

/** `ti4_view::planning::draft::DraftStep`: one entry of a draft script. */
export type DraftStep = import("./movementPlan").PlanStep | { kind: "choose"; option_id: string };

/**
 * `ti4_view::planning::audit::PlanScope`: what a draft script is of. The next tactical action of
 * the seat, or its secondary of the strategic action that is resolved (`secondary` of the update).
 */
export type DraftScope =
  | { kind: "tactical" }
  | { kind: "secondary"; primary: string; card: string };

/** `ti4_view::planning::draft::DraftStop`: why a run of a draft ended. */
export type DraftStop =
  /** The script is used up: the question of the update is open. */
  | { kind: "open" }
  /** The game played the whole script, and the tactical action or the secondary is over. */
  | { kind: "complete" }
  /** The game does not offer what step `step` of the script asks for. */
  | { kind: "refused"; step: number; reason: string }
  /** Something a draft cannot show: `ti4_view::planning::audit::StopReason`. */
  | { kind: "stopped"; reason: string }
  | { kind: "failed"; reason: string };

/** `ti4_view::planning::draft::DraftOutcome` */
export interface DraftOutcome {
  /** Where the run ended. It has the open question when the stop is "open". */
  update: SessionUpdate;
  /** The question of the first ship to move, before any ship moved. */
  movement?: SessionUpdate;
  /** The question of the first ground force to land: what the landings are staged against. */
  invasion?: SessionUpdate;
  /** The question of the first unit to produce: what a production is staged against. */
  production?: SessionUpdate;
  /** How many steps of the script the game took, or did not need. */
  consumed: number;
  stop: DraftStop;
}

/** `ti4_view::projection::SessionUpdate`: the fields of `StateUpdateMsg` that web2 reads. */
/** `ti4_view::seats::SeatFact`: who sits in a seat of a running game. It is public. */
export interface SeatFact {
  /** The name of the player at the table. */
  nickname: string;
  /** "human" is a person, "smart" the rule bot, "random" a seat that decides at random. */
  kind: "human" | "smart" | "random";
}

export interface SessionUpdate {
  viewer: ViewerRole;
  view: GameView;
  pending_choice?: PendingChoiceEnvelope;
  turn_status: PublicTurnStatus;
  /** What the engine says about the pending choice of a tactical action. Only the seat that is asked has it. */
  tactical?: TacticalFacts;
  /** The exploration decks and the planets that are still explored. */
  exploration?: ExplorationFacts;
  /** The action card deck, its discard pile and the hand limit. */
  action_cards?: ActionCardFacts;
  /** The secret objective deck and what each seat has scored of it. */
  secret_objectives?: SecretObjectiveFacts;
  /** Where the victory points of every seat come from. */
  score?: ScoreFacts;
  /** The agenda deck. The laws in play are in the table of the view. */
  agendas?: AgendaFacts;
  /** The strategic action that is resolved now, from the card that is played to its last follower. */
  secondary?: SecondaryFacts;
  /** The reaction window that the viewer is asked in, with what it offers as printed. */
  reaction?: ReactionFacts;
  /** Who sits in each seat, by the id of its player. A game with no table has none. */
  seats?: Record<string, SeatFact>;
}

/**
 * When a reaction is offered to a seat: always, never, or only in the turns of the seat
 * (`ti4_model::state::ReactionMode`). "Only in my turns" is of the action phase.
 */
export type ReactionMode = "always" | "never" | "ownturn";

/** The modes that a seat set, by the key of the reaction. A key that is not there is "always". */
export type ReactionModes = Record<string, Exclude<ReactionMode, "always">>;

/** `ti4_engine::decision_context::DecisionTrigger`: what opened a reaction window. All of it is public. */
export interface DecisionTrigger {
  kind: string;
  /** The event of the engine: "TACTICAL_ACTION_ENDED". */
  event_type: string;
  relation: "when" | "after";
  /** The seat that caused the event. */
  actor?: string;
  /** A second seat: a partner, the victim of hits, a defender. */
  subject?: string;
  /** The id of an action card that was played, or of a strategy card. */
  card?: string;
  agenda?: string;
  system?: string;
  planet?: string;
  hits?: number;
}

/** `ti4_view::reaction::ReactionOffer`: one thing that a reaction window offers, as printed. */
export interface ReactionOffer {
  /** The id of the option that uses it. A slot of several action cards has one offer for each name. */
  option: string;
  /** What the seat mutes: "leader:nomadagentmercer", or the printed name of an action card. */
  key: string;
  kind:
    | "leader"
    | "ability"
    | "technology"
    | "promissory"
    | "unit"
    | "breakthrough"
    | "action_card";
  /** Of a leader: "agent", "commander" or "hero". */
  role?: string;
  name: string;
  /** When it is used, as printed. */
  window?: string;
  text?: string;
}

/** `ti4_view::reaction::ReactionFacts` */
export interface ReactionFacts {
  trigger?: DecisionTrigger;
  /** The window is in the action phase: "only in my turns" says something about it. */
  own_turn_applies: boolean;
  offers: ReactionOffer[];
}

/** `ti4_view::secondary::SecondaryFacts`: who played which strategy card, and what each follower did. */
export interface SecondaryFacts {
  primary: string;
  card: string;
  /** The printed name of the card. */
  name: string;
  /** The primary is resolved: the followers are asked, in their order. */
  open: boolean;
  /** The seats that may follow, in the order in which they are asked. */
  followers: { seat: string; state: "waiting" | "followed" | "passed" | "ineligible" }[];
  /** The viewer may draft its secondary now. */
  draft: boolean;
}

// The messages of the websocket (`crates/ti4-server/src/protocol`), as far as web2 reads them.

/** `ti4_server::protocol::PROTOCOL_VERSION`. Every message has it. */
export const PROTOCOL_VERSION = 3;

/** `ti4_view::status::RejectionReason` */
export type RejectionReason =
  | { reason: "stale_version"; expected: number; current: number }
  | { reason: "stale_nonce" }
  | { reason: "unauthorized_seat"; seat: string | null }
  | { reason: "no_pending_choice" }
  | { reason: "unknown_option"; option_id: string }
  | { reason: "validation_failed"; message: string }
  /** `UndoBarrier`: the decision that the seat asked to take back lies behind it. */
  | { reason: "undo_barrier"; barrier: "revealed" | "random" | "other_seat" }
  /**
   * `AnsweredElsewhere`: another tab or device of the seat answered this choice already, or the
   * server sent a ready draft of the seat for it. Only to a page that asked for the staged work.
   */
  | { reason: "answered_elsewhere" };

/**
 * `InitialSnapshotMsg` and `StateUpdateMsg`: a `SessionUpdate` with the version of the game.
 * Not read for now: `state`, `galaxy_layout`, `events`, `current_path`, and `cursor`,
 * `redo_count` and `generation` of `history` (the history of the host).
 */
export interface StateMsg extends SessionUpdate {
  protocol_version: number;
  game_id: string;
  game_version: number;
  /** The reactions that the seat of this connection set, by key. Absent when it set none. */
  reaction_modes?: Record<string, ReactionMode>;
  /**
   * `AutoResolvedNote`, of a `state_update` only. Read for `passed` alone: the cards of a
   * window that was declined because the seat set each of them to never offer.
   */
  auto_resolved?: { id: string; passed?: string[] }[];
  /** Of an `initial_snapshot` only: the digest of the corpus of the server (`Catalog.content`). */
  content?: string;
  /**
   * `HistoryStatus`, read for `can_undo` alone: the seat of this connection may take back its
   * last decision (`undo`). Absent when it may not.
   */
  history?: { can_undo?: boolean };
  /**
   * Of an `initial_snapshot` only: the steps of a plan that the undo of this seat took back,
   * in order. The server tells them once, with the first snapshot after the undo.
   */
  undone?: import("./movementPlan").PlanStep[];
}

/** `ScriptStopped`: where a script that was sent stopped, counted in entries of the script. */
export interface ScriptStopped {
  applied: number;
  remaining: DraftStep[];
  interrupted: boolean;
  reason: string;
}

/**
 * `ti4_server::protocol::ServerMessage`. Not read for now, and so not named here: `event`,
 * `pending_choice` (an update has it), `action_accepted`, `game_over` (the update before it says
 * so), `pong`, and the messages of the planning runner (`planning_update`, `planning_status`,
 * `planning_result`).
 */
export type ServerMessage =
  | ({ type: "initial_snapshot" } & StateMsg)
  | ({ type: "state_update" } & StateMsg)
  /**
   * Who the game waits for, when nothing else of the game changed: a seat that the server plays
   * takes its time over a decision.
   */
  | {
      type: "turn_status";
      protocol_version: number;
      game_version: number;
      status: PublicTurnStatus;
    }
  | { type: "action_rejected"; protocol_version: number; reason: RejectionReason }
  | {
      type: "error";
      protocol_version: number;
      /** `ErrorKind`: "unauthorized", "malformed_message", "unsupported_version", ... */
      kind: string;
      message: string;
    }
  | {
      type: "draft_outcome";
      protocol_version: number;
      request_id: string;
      base_version: number;
      outcome: DraftOutcome;
    }
  | { type: "draft_applied"; protocol_version: number; nonce: string; stopped?: ScriptStopped }
  | ({ type: "seat_staged" } & SeatStagedMsg)
  | { type: "other"; protocol_version: number };

/** `ti4_host::protocol::staged::StagedSlot`: one kind of staged work of a seat. */
export type StagedSlot = "movement" | "plan" | "secondary";

/** `ti4_host::protocol::staged::StagedValue`: a slot and the revision of its last write. */
export interface StagedValue {
  revision: number;
  /** Null or absent: the slot was cleared. */
  value?: unknown;
}

/**
 * `ti4_host::protocol::staged::SeatStagedMsg`: what the seat of this connection has staged, sent
 * to a connection that subscribed with `staged`: after the snapshot, after every write of any
 * connection of the seat, and as the answer to a write that was not taken. It has no game
 * version: the newest is the one with the highest `revision`.
 */
export interface SeatStagedMsg {
  protocol_version: number;
  game_id: string;
  /** The revision of the newest write of any slot (0: never written). */
  revision: number;
  /** The slots that were ever written. */
  slots: Partial<Record<StagedSlot, StagedValue>>;
  /** The connection whose write this is. */
  writer?: number;
  /** The id of this connection, in the message after the snapshot only: the `writer` of its writes. */
  connection?: number;
}

/** `ti4_server::protocol::ClientMessage`, without the messages of the planning runner. */
export type ClientMessage = ClientRequest & { protocol_version: number };

/** A message to the server, before it has the version of the protocol. */
export type ClientRequest =
  | {
      type: "subscribe";
      game_id: string;
      player_session?: string;
      /** The connection takes part in the staged work of its seat (`seat_staged`, `put_staged`). */
      staged?: boolean;
    }
  | {
      type: "submit_choice";
      game_id: string;
      nonce: string;
      expected_version: number;
      option_id: string;
    }
  | { type: "set_reaction_mode"; game_id: string; card: string; mode: ReactionMode }
  | {
      type: "run_draft";
      game_id: string;
      request_id: string;
      script: DraftStep[];
      /** Left out: the next tactical action. */
      scope?: DraftScope;
    }
  | {
      type: "apply_draft";
      game_id: string;
      nonce: string;
      expected_version: number;
      script: DraftStep[];
    }
  /** Takes back the last decision of the seat. The server then closes the socket as replaced. */
  | { type: "undo"; game_id: string; expected_version: number }
  /**
   * Writes a slot of the staged work of the seat, when `base_revision` is its revision
   * (`ti4_host::protocol::client::ClientMessage::PutStaged`). No value clears it.
   */
  | {
      type: "put_staged";
      game_id: string;
      slot: StagedSlot;
      base_revision: number;
      value?: unknown;
    }
  | { type: "ping"; sequence: number };

// ---- The table of a game on the server, before and while it runs --------------------------------

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

/** `ti4_server::maps::MapChoiceView` */
export interface MapChoiceView {
  kind: "template" | "random";
  alias?: string | null;
  name?: string | null;
  author?: string | null;
  systems: number;
  hyperlanes: boolean;
  recommended: boolean;
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
  /** How the seats are given at the start: "random" (drawn from the seed) or "join". */
  seat_order: string;
  /** The wait before each answer of a seat that the server plays, in milliseconds. */
  bot_delay_ms: number;
  /** The scenario that the game starts from. Absent: round 1. */
  start_from?: string;
}

/** `ti4_server::session::table::SetupOptions`: what a host can choose, with the names. */
export interface SetupOptions {
  min_players: number;
  max_players: number;
  strategy_card_sets: {
    id: string;
    name: string;
    /** The table plays this set when it names none. */
    default: boolean;
    /** The eight cards, by initiative. */
    cards: { id: string; name: string; initiative: number }[];
  }[];
  faction_groups: { name: string; factions: { id: string; name: string }[] }[];
  bot_kinds: { id: string; name: string; description: string }[];
  max_bot_delay_ms: number;
  default_bot_delay_ms: number;
  /** `ti4_view::scenario::Scenario`: the games that a table can start from. */
  scenarios: { id: string; title: string; description: string; seed: number; players: number }[];
}

/** `ti4_server::maps::MapPreview`: the board that the table will play. */
export interface MapPreview {
  player_count: number;
  tiles: BoardTileView[];
  /** The place of a seat: `home_system_id` is a tile of `tiles`, not a system of the game. */
  seats: { seat: number; home_system_id: string }[];
}

/** `ti4_view::maps::MapTemplateSummary`: a map that a table can play. */
export interface MapSummary {
  alias: string;
  name?: string;
  author: string;
  player_count: number;
  buildable: boolean;
  systems: number;
  hyperlanes: boolean;
  /** The map that a table of this size starts with. */
  recommended: boolean;
}

/** `ti4_view::maps::MapChoice`: the map of a table. */
export type MapChoice = { kind: "template"; alias: string } | { kind: "random" };

/**
 * `ti4_host::setup::TableSetup`: what a host chose for a table. Both hosts start a table from
 * it, so a table of a seed is the same game in the page and on the server.
 */
export interface TableSetup {
  player_count: number;
  seed: number;
  /** Absent: the seeded random board. */
  map?: MapChoice;
  /** The id of a set of `SetupOptions.strategy_card_sets`. Absent: the usual set. */
  strategy_cards?: string;
  /** Absent: "random", drawn from the seed. */
  seat_order?: "random" | "join";
  /** Absent: "random", drawn from the seed. "seat" is the first seat. */
  speaker_order?: "random" | "seat";
  /** The id of a scenario. Absent: round 1. */
  start_from?: string | null;
}

/** One player of the table of the page (`ti4-wasm`, `SeatRequest`). */
export interface TableSeat {
  /** "human" is played in the page, "smart" by the rule bot, "random" at random. */
  kind: "human" | "smart" | "random";
  /** The faction that was chosen for the player. Absent: drawn from the seed. */
  faction?: string | null;
  /** The name of the player at the table. With a name for every player the game says them (`SessionUpdate.seats`). */
  name?: string;
}

/**
 * The table of the page as the setup exports of the engine read it (`ti4-wasm`,
 * `TableRequest`). `seats` are the players in the order in which they came; the player who came
 * as number i has the id of the i-th letter ("a", "b", …), wherever the draw seats it.
 */
export interface TableRequest {
  setup: TableSetup;
  seats: TableSeat[];
}

/** `ti4_setup_options`: the options of the server, and the maps of the number of seats. */
export interface TableOptions extends SetupOptions {
  maps: MapSummary[];
}

/** `ti4_setup_preview`: the board of the table, and what the table says of its map. */
export interface TablePreview extends MapPreview {
  map: MapChoiceView;
}

/** `ti4_setup_draw`: what the start draws for a table. */
export interface TableDraw {
  /** The ids of the players in the order of their seats. */
  players: string[];
  /** For each seat, the place of its player in the order in which the players came. */
  order: number[];
  /** The faction of each seat. For a scenario: the factions that it seats, in the order of the join. */
  factions: string[];
}

/** `ti4_server::ws::table::TableServerMessage` */
export type TableServerMessage =
  | { type: "table"; protocol_version: number; viewer?: string; lobby: LobbyState }
  | { type: "pong"; protocol_version: number }
  | { type: "error"; protocol_version: number; kind: string; message: string };
