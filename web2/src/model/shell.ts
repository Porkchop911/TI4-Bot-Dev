import type { ActionView } from "./action";
import type { BoardView, ExplorationDeck } from "./board";
import type { OfferView, Rich, SeatId, SeatView } from "./core";
import type { Intent } from "./intents";

export type Workspace = "live" | "draft" | "history";

export interface ToolbarView {
  round: number;
  /** The turn of the action phase (LRR 3), from 1; 0 outside the action phase. */
  turn: number;
  phase: string;
  /** "Tactical action complete", or what the game waits for. */
  status: string;
  workspace: Workspace;
  /** False when the viewer may not draft, with the reason. */
  draftLocked: string | null;
  past: { label: string } | null;
  /** Controls of the private draft. Null outside the draft. */
  draft: { canUndo: boolean; canRedo: boolean; canApply: boolean; applyHint: string } | null;
}

/** A card in the play area of a player, as printed, with its state. */
export interface ComponentCardView {
  name: string;
  /** "Agent · Master Tactician", "Faction ability", "Technology", "Relic", "Legendary planet · Primor". */
  kind: string;
  /** When the card is used, as printed: "ACTION:", "At the start of a ground combat round:". */
  window: string;
  text: string;
  /** Null for a card that has no state: a faction ability, a relic that always holds. */
  state: "ready" | "exhausted" | "locked" | "purged" | null;
  /** How a locked leader is unlocked, as printed. */
  unlock: string;
  /** Of a card of the viewer that a reaction window offers: when it is offered. */
  offer?: OfferView | null;
}

/** One row of the player table. Compare `PlayerView`. */
export interface PlayerRowView {
  seat: SeatId;
  isMe: boolean;
  turn: "now" | "next" | null;
  speaker: boolean;
  passed: boolean;
  victoryPoints: number;
  /** One card for each player with five or more players, two with four or fewer. In initiative order. */
  strategyCards: { number: number; name: string; used: boolean }[];
  resources: [ready: number, total: number];
  influence: [ready: number, total: number];
  tradeGoods: number;
  commodities: [held: number, limit: number];
  tokens: [tactic: number, fleet: number, strategy: number];
  actionCards: number;
  secrets: [scored: number, held: number];
  planets: number;
  abilities: ComponentCardView[];
  technologies: string[];
  /** Agent, commander, hero. */
  leaders: ComponentCardView[];
  /** The other cards that the player uses: technologies with an action, relics, legendary planets. */
  components: ComponentCardView[];
  /** Relic fragments in the play area, by deck. Decks with none are left out. */
  fragments: { deck: ExplorationDeck; count: number }[];
  /**
   * Of the viewer: the reactions that are not always offered and that no card of the sheet has
   * (a promissory note, a unit, a breakthrough). The sheet is where they are set back.
   */
  muted?: OfferView[];
}

/** The open task lets the viewer choose a player. The player table is the picker. */
export interface PlayerPickView {
  /** "Choose", for the label of a row. */
  verb: string;
  /** The seats in the choice. A reason says why a seat cannot be chosen. */
  seats: Record<SeatId, { reason: string | null; note?: string }>;
  /** One seat, or several when the task allows it. */
  chosen: SeatId[];
}

export interface PlayerTableView {
  pick: PlayerPickView | null;
  rows: PlayerRowView[];
  freeCards: { number: number; name: string; tradeGoods: number }[];
}

export interface ListSectionView {
  title: string;
  rows: Rich[];
}

export interface LogEntryView {
  id: string;
  seat: SeatId;
  type: string;
  summary: string;
  stages: { name: string; text: string }[];
  /** The action that is open in Live. */
  current: boolean;
}

/** One card of an exploration deck. Cards of one name are one row. */
export interface ExplorationCardView {
  name: string;
  /** "Instant", "Fragment", "Attach" or "Token". */
  kind: string;
  text: string;
  left: number;
  total: number;
  /** Who drew a card of this name, and where: "Round 2 · Sol · Vega Major". */
  drawn: string[];
}

/** One exploration deck: what it holds, and who has its relic fragments. */
export interface ExplorationDeckView {
  deck: ExplorationDeck;
  title: string;
  left: number;
  total: number;
  fragments: { seat: SeatId; count: number }[];
  cards: ExplorationCardView[];
}

/** One action card as printed. Copies of a card are one card here. */
export interface ActionCardView {
  name: string;
  /** "ACTION:", "At the start of a combat round:". */
  window: string;
  text: string;
  /** The card is a component action: no window offers it. */
  action: boolean;
  /** When the card is played, roughly: "Action", "Combat", "Agenda phase". */
  group: string;
}

/** One card of the action card deck, with where its copies are. */
export interface DeckCardView extends ActionCardView {
  /** How many copies the deck begins a game with. */
  total: number;
  discarded: number;
  /** In the hand of the viewer. */
  held: number;
  /** Not seen: in the deck or in the hand of another player. The total without the two above. */
  open: number;
  /** The viewer acts as if holding it. */
  faked: boolean;
}

/** The action cards of the viewer and of the deck. */
export interface ActionCardsView {
  held: number;
  /** The hand limit of the viewer. Null: there is none, or the game does not say. */
  limit: number | null;
  /** Null when the game does not say. */
  deck: { left: number; total: number; discarded: number } | null;
  /** `offer`: when a window offers the card to the viewer. Null for a card that no window offers. */
  hand: (ActionCardView & { copies: number; offer: OfferView | null })[];
  /** The cards that the viewer acts as if holding. Never a card that is a component action. */
  faked: ActionCardView[];
  /** Every card of the deck, by name, in groups by when a card is played. */
  reference: { group: string; cards: DeckCardView[] }[];
}

/** One secret objective as printed. */
export interface SecretObjectiveView {
  name: string;
  /** When it is scored: "Status phase", "Action phase", "Agenda phase". */
  phase: string;
  text: string;
  /** What the objective asks for, roughly: "Ships on the board", "Win a combat". */
  group: string;
}

/** One secret objective of the deck, with where it is as far as the viewer knows. */
export interface DeckSecretView extends SecretObjectiveView {
  /** `unseen`: in the deck or in the hand of another player. `held`: in a hand that the viewer sees. */
  state: "unseen" | "held" | "scored";
  /** The seat that holds or scored it. Null for an unseen objective. */
  by: SeatId | null;
  /** `by` is the viewer. */
  mine: boolean;
}

/** The secret objectives of the viewer and of the deck. */
export interface SecretsView {
  /** In the hand of the viewer, not scored. */
  held: number;
  scored: number;
  /** How many the viewer may have, scored and unscored together. Null when the game does not say. */
  limit: number | null;
  /** The objectives of the viewer: the hand, then the scored ones. */
  mine: (SecretObjectiveView & { scored: boolean })[];
  /** How many objectives the deck begins a game with. */
  total: number;
  /** Not in the hand of the viewer and not scored by anyone. */
  unseen: number;
  /** How many cards the deck still holds. Null when the game does not say. */
  deck: number | null;
  /** Every objective of the deck, in groups by what an objective asks for. */
  reference: { group: string; unseen: number; cards: DeckSecretView[] }[];
}

/** One agenda as printed. */
export interface AgendaView {
  name: string;
  /** "Law" or "Directive", and what the vote is about: "Law · Elect Player". */
  kind: string;
  /** The printed text: the outcomes one line each, or of a law in play the outcome that passed. */
  text: string;
}

/** A law in play, with the outcome that passed. */
export interface LawView extends AgendaView {
  /** The elected seat, or the outcome in words: "For", "Against", the name of what was elected. */
  outcome: { seat: SeatId } | { text: string };
}

/** The laws in play and the rest of the agenda deck. */
export interface AgendasView {
  /** The laws in play, by name. */
  laws: LawView[];
  /** How many agendas of the deck that a game begins with are not in play. */
  rest: number;
  /** How many cards the deck still holds. Null when the game does not say. */
  deck: number | null;
  /** Every agenda that is not in play, in groups by what an agenda affects. */
  reference: { group: string; cards: AgendaView[] }[];
}

/** What one seat has from one source of points. */
export type ScoreCellView =
  | { kind: "none" }
  /** The seat scored the objective of the row. */
  | { kind: "scored" }
  /**
   * `from`: the seats whose Support for the Throne gives the points. Empty for another source.
   * `detail`: the cards behind the points as printed, when the game says them (the scored secret
   * objectives). `text` is empty without the catalogue.
   */
  | { kind: "points"; value: number; from: SeatId[]; detail: { name: string; text: string }[] };

/** One source of points: a public objective, or another source ("Custodians of Mecatol Rex"). */
export interface ScoreRowView {
  name: string;
  /** The printed text of an objective, an agenda, a relic or a card. Empty when the source has none. */
  text: string;
  /** What a public objective is worth. Null for another source: its cells say the points. */
  worth: number | null;
  /**
   * A point can still be scored here, or change hands: an objective that not every seat scored,
   * the custodians while no seat has them, Support for the Throne, and the points of an agenda
   * (a law can be repealed). The Objectives sheet shows
   * these rows only; the sheet of a player has every row of its seat.
   */
  open: boolean;
  /** One cell for each seat, in the order of `ScoreView.seats`. */
  cells: ScoreCellView[];
}

/** Where the victory points of every seat come from: each source against each seat. */
export interface ScoreView {
  /** The points that win the game. */
  target: number;
  /** The viewer first, then the other seats by their points, the most first. */
  seats: { seat: SeatId; points: number; isMe: boolean }[];
  /** "Public objectives · stage I", "Public objectives · stage II", "Other points". */
  groups: { title: string; rows: ScoreRowView[] }[];
  /**
   * The public objectives that the game can still reveal, in groups by what an objective asks
   * for ("Spend", "Planets you control"). A game draws five of each stage, and nobody knows
   * which. Stage I is not there once an objective of stage II is revealed. Empty without the
   * catalogue.
   */
  deck: { group: string; cards: { name: string; text: string; worth: number }[] }[];
}

export interface ReferenceView {
  /** Null when the game does not say where the points come from. */
  objectives: ScoreView | null;
  technology: ListSectionView[];
  cards: ListSectionView[];
  /** The action cards: the hand of the viewer and the deck. Null for a viewer with no hand. */
  actionCards: ActionCardsView | null;
  /** The secret objectives: those of the viewer and the deck. Null for a viewer with no hand. */
  secrets: SecretsView | null;
  /** The laws in play and the agenda deck. */
  agendas: AgendasView;
  /** The exploration decks, in the order cultural, hazardous, industrial, frontier. */
  exploration: ExplorationDeckView[];
  /** `current` is the round in play: the log shows it open. */
  log: { round: number; label: string; current: boolean; entries: LogEntryView[] }[];
}

export interface ApplyView {
  items: string[];
  cost: string;
  note: string;
}

export interface ShellView {
  seats: Record<SeatId, SeatView>;
  /** Colours the frame: a private draft, the live game, or a complete action. */
  accent: "draft" | "live" | "done";
  toolbar: ToolbarView;
  players: PlayerTableView;
  board: BoardView;
  action: ActionView;
  reference: ReferenceView;
  /** What "Apply to Live" will commit. Null when there is nothing to apply. */
  apply: ApplyView | null;
  /** A list row to scroll into view, after the player chose its counterpart on the board. */
  reveal: string | null;
  toast: { id: number; text: string } | null;
  announcement: string;
  /** The viewer has a decision open: a question of the game, or a draft. */
  asked: boolean;
  /**
   * The game is asked what follows from a draft of the viewer and has not answered yet: the
   * screen has nothing to decide, and the next step of the viewer is not known.
   */
  settling?: boolean;
  /**
   * The viewer's answer to the open question is sent, and the game has not answered it yet: the
   * question on the screen is no longer open, and the next step of the viewer is not known.
   */
  answered?: boolean;
}

/**
 * What the game of a session can do besides play: the tools for testing a local game. A game on
 * the server has none. A control for one of them is shown from here, never from the kind of game.
 */
export interface Capabilities {
  /** The game can wait before each decision of a seat that is not played here. */
  stepping: boolean;
  /** Another seat can be played here. */
  switchSeat: boolean;
  /** The game can start from a prepared board. */
  scenarios: boolean;
  /** The game is kept in this page: it can be started again, exported and imported. */
  save: boolean;
}

/** The only thing the game shell needs. The mock implements it now; a websocket session will later. */
export interface GameSession {
  view: ShellView;
  dispatch: (intent: Intent) => void;
  capabilities: Capabilities;
}
