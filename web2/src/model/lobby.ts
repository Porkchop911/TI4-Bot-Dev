import type { BoardView } from "./board";
import type { HelpView, SeatId, SeatView } from "./core";
import type { ComponentCardView } from "./shell";

/** A change of the setup of a table. A part that is left out stays as it is. */
export interface LobbySettingsChange {
  players?: number;
  /** A set of strategy cards, by its id. */
  cards?: string;
  /** The wait of the seats that the server plays, in milliseconds. */
  waitMs?: number;
  /** The id of a scenario, or null for round 1. */
  startFrom?: string | null;
}

/** What a player does in the lobby. A change applies at once: nothing is staged. */
export type LobbyIntent =
  /** Takes an open seat, as the player of this tab. */
  | { type: "join" }
  /** Takes an open seat as another player, for another tab of this browser. */
  | { type: "joinTab" }
  /** Makes this tab a player that this browser has at the table. */
  | { type: "become"; player: string }
  /** The bot that "+ Bot" and "Fill" put in a seat. A choice of this page, not of the table. */
  | { type: "chooseBot"; kind: string }
  /** Puts a bot of the kind in the first open seat; a table in the page adds a seat when none is open. */
  | { type: "addBot"; kind: string }
  | { type: "fillBots"; kind: string }
  | { type: "remove"; player: string }
  /** The host puts the players in another order of the rows: the seats of the game. */
  | { type: "shuffleSeats" }
  /** The host changes the setup. */
  | { type: "set"; change: LobbySettingsChange }
  /** The viewer takes another name. */
  | { type: "rename"; name: string }
  /** The faction of a seat: of the viewer, or of a seat that the server plays. Null: random. */
  | { type: "faction"; player: string; faction: string | null }
  | { type: "start" }
  /** A visitor of a running game opens the game, with no seat. */
  | { type: "watch" }
  /** A visitor takes the seat of a person who is away, under the name of the seat. */
  | { type: "takeOver"; player: string; name: string }
  | { type: "inspectSystem"; system: string }
  | { type: "closeInspector" };

/** The one sign of a row, with its word (the tooltip and the accessible name). */
export interface LobbySignView {
  kind: "host" | "rule" | "random" | "model" | "away" | "open";
  word: string;
}

export interface LobbyButtonView {
  label: string;
  /** What the button does, when the label does not say it. */
  title?: string;
  intent: LobbyIntent;
}

/** A faction as its sheet prints it. It is behind the name of the faction. */
export interface FactionSheetView {
  /** The name as printed: "The Federation of Sol". */
  title: string;
  /** The values of the sheet, one line each: the home system, the commodities, what it begins with. */
  facts: { label: string; value: string }[];
  abilities: ComponentCardView[];
  /** The faction technologies. */
  technologies: ComponentCardView[];
  leaders: ComponentCardView[];
}

/** "+ Bot" or "Fill". Null intent: it cannot be done now, and `title` says why. */
export interface LobbyBotActionView {
  label: string;
  /** What it does, or why it cannot be done. */
  title: string;
  intent: LobbyIntent | null;
}

/**
 * The row under the seats: one choice of the bot (the rule bot, a random seat, a model), and
 * "+ Bot" and "Fill", which put it in a seat. With no seat to fill it is there, disabled.
 */
export interface LobbyBotsView {
  /** The sign of the chosen kind, in the column of the signs of the seats. */
  sign: LobbySignView;
  /** The id of the chosen kind. */
  chosen: string;
  options: { id: string; label: string; intent: LobbyIntent }[];
  /** The list can be changed: false when neither action can be done. */
  enabled: boolean;
  add: LobbyBotActionView;
  fill: LobbyBotActionView;
}

/** One choice of the faction of a seat. The row is the control, and the choice applies at once. */
export interface LobbyFactionOptionView {
  /** The id of the faction. Empty: "Random". */
  id: string;
  name: string;
  /** The seat has this one ("▸"). */
  chosen: boolean;
  /** Null: the seat has it already, or another seat has it and `taken` says who. */
  intent: LobbyIntent | null;
  /** The name of the other seat that has the faction, or empty. */
  taken: string;
  /** Null for "Random", and without the catalogue. */
  sheet: FactionSheetView | null;
}

/** The choice of the faction of a seat: "Random" first, then the factions in their groups. */
export interface LobbyFactionEditorView {
  /** The accessible name: "Faction of Ada". */
  label: string;
  random: LobbyFactionOptionView;
  /** A group with no name has no heading. */
  groups: { name: string; options: LobbyFactionOptionView[] }[];
}

/**
 * One seat of the table, in the order of the seats of the game: the order in which the players
 * came, until the host shuffles the seats. The list has no numbers.
 */
export interface LobbyRowView {
  id: string;
  /** The player of the seat. Of a running game it is the seat of the game. Null: the seat is open. */
  player: SeatId | null;
  sign: LobbySignView | null;
  /** The nickname, or "Open". */
  name: string;
  /** The row of the viewer. */
  you: boolean;
  /** The viewer can change the name here ("✎"). */
  rename: boolean;
  /** Another player of this browser. The row is the control: the tab becomes that player. */
  become: LobbyIntent | null;
  /** The name of the faction, or "Random". Empty for an open seat. */
  faction: string;
  /** The sheet of the chosen faction, behind its name. Null for "Random". */
  sheet: FactionSheetView | null;
  /**
   * The row opens into the choice of its faction: the row of the viewer, and for the host a row
   * of a seat that the server plays. Null: the row is no control.
   */
  editor: LobbyFactionEditorView | null;
  /** The controls of the row: "Join in a new tab", "Rejoin". */
  buttons: LobbyButtonView[];
  /** Removes the seat. Shown as "×". */
  remove: LobbyButtonView | null;
}

/** One step of a counter of the setup. */
export interface LobbyStepView {
  /** The accessible name: "One player fewer". */
  label: string;
  /** Null: the step cannot be taken, and `reason` says why. */
  intent: LobbyIntent | null;
  reason: string;
}

/** The control of a value of the setup: an amount in steps, or one of a list. */
export type LobbySetupControlView =
  | { kind: "counter"; less: LobbyStepView; more: LobbyStepView }
  | { kind: "list"; options: { id: string; label: string; intent: LobbyIntent }[]; chosen: string };

/** One value of the setup. Only a viewer who may change it has a control. */
export interface LobbySetupRowView {
  id: string;
  label: string;
  value: string;
  control: LobbySetupControlView | null;
  /** What the choices of the row are or do. Behind "?". */
  help: HelpView[];
}

export interface LobbyFooterView {
  /** Why the main button waits, or who the table waits for. */
  note: string;
  /** The main button. Enter is this button. Null: the viewer has none. */
  main: { label: string; disabled: boolean; intent: LobbyIntent } | null;
  /** A visitor gives a name before the join. */
  nickname: boolean;
}

/**
 * The lobby of a table: the map, the players and the setup. The same screen for everyone, and
 * for the table of a running game, where nothing but the wait has a control.
 */
export interface LobbyView {
  /** "Lobby · 6 players"; of a running game "Table · 6 players". */
  title: string;
  /** "Offline · reconnecting", or empty. */
  status: string;
  board: BoardView;
  /** The seats that the board draws: the place of a seat, not yet given to a player. */
  seats: Record<SeatId, SeatView>;
  setup: LobbySetupRowView[];
  /** "3 of 6" */
  filled: string;
  /** What the kinds of a seat that the server plays do. Behind "?". */
  help: HelpView[];
  rows: LobbyRowView[];
  /** The bot that "+ Bot" and "Fill" put in a seat, for the host before the start. */
  bots: LobbyBotsView | null;
  /** "Shuffle seats", for the host before the start: the rows are the seats of the game. */
  shuffle: LobbyButtonView | null;
  footer: LobbyFooterView;
}
