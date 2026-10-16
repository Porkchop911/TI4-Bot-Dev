import type { PlanetTraitsView, SystemDetailsView } from "./board";
import type {
  CounterView,
  Force,
  GaugeView,
  HelpView,
  LinkToken,
  OfferView,
  PillView,
  PlanetId,
  Rich,
  SeatId,
  SystemId,
  Tone,
  UnitType,
} from "./core";
import type { TechColor } from "./board";
import type { Intent } from "./intents";

export type StepStatus =
  | "future"
  | "skipped"
  | "done"
  | "decision"
  | "needs-review"
  | "battle"
  | "draft";
export type TrailStatus = "complete" | "active" | "todo" | "skipped";

export interface StepTabView {
  name: string;
  status: StepStatus;
  caption: string;
  /**
   * The tab is no control: a resolved step of the live game, whose result is in the live turn of
   * the Log.
   */
  locked?: boolean;
}

/**
 * Whom a part of the open action serves. "decide": what the viewer needs for the choice that is
 * open now (the question, the source that asks, the options, the costs, what is staged, the odds):
 * the step sheet on the map. "inform": what happened, or what the other seats do (their rows in a
 * strategic action, "Then: …", the turn of another seat, a combat that the viewer watches): the
 * live turn of the Log. The selectors set it; a block without it decides.
 */
export type BlockRole = "decide" | "inform";

/** A block informs: it is of the Log while the viewer decides, never of the step sheet. */
export const informs = (block: { role?: BlockRole }): boolean => block.role === "inform";

/** A button that the data source offers. Compare `ChoiceOptionDto`. */
export interface ActionButtonView {
  label: string;
  tone?: "default" | "primary" | "quiet";
  disabled?: boolean;
  pressed?: boolean;
  arrow?: boolean;
  /** Shortcut key: a letter, a digit, "Enter" or "Escape". The shell listens for it and the button shows it. */
  key?: string;
  intent: Intent;
}

export interface FooterView {
  note: string;
  error: boolean;
  actions: ActionButtonView[];
  /** The open payment, as one line: what pays and the total. Its controls are on the board. */
  payment?: PaymentView;
}

// ---- Dice ---------------------------------------------------------------------------------------

/** Compare `CombatDieRoll`. */
export interface DieView {
  roll: number;
  hit: boolean;
}

export interface BattleRowView {
  /** Null for dice whose units the game does not name: `label` says what rolled. */
  unit: UnitType | null;
  /** The name of the row, when it is not the name of its unit. */
  label?: string;
  /** Null when the units are not known. */
  count: number | null;
  /** Shown as "7+". Null when the unit does not roll. */
  target: number | null;
  targetTitle: string;
  modified: boolean;
  /** Rolled dice, or the number of dice that will roll. */
  dice: DieView[] | number;
  /** "2 hits", or "≈1.4" before the roll. */
  hits: string;
  scored: boolean;
  pills: { tone: "damage" | "loss" | "staged"; label: string; title?: string }[];
}

export interface BattleSideView {
  seat: SeatId;
  role: "Attacker" | "Defender";
  isViewer: boolean;
  hits: number | null;
  rows: BattleRowView[];
}

/** One table for every dice step: space cannon, barrage, combat rounds, bombardment, ground combat. */
export interface BattleTableView {
  label: string;
  score: string;
  owed: { hits: number; seat: SeatId } | null;
  sides: BattleSideView[];
}

export interface OddsView {
  kind: "space" | "ground";
  attacker: SeatId;
  defender: SeatId;
  /** Percentages. */
  attackerWins: number;
  defenderWins: number;
  rollouts: number;
  rounds: string;
  attackerLeft: string;
  defenderLeft: string;
  caveat: string;
}

/**
 * The odds of a combat that the staged units start, as `ti4-calc` computes them in the page. The
 * attacker is the seat that moves or lands.
 */
export interface CombatOddsView {
  defender: SeatId;
  /** Whole percentages of the attacker, which sum to 100. Null before the first answer. */
  win: number | null;
  /** Both sides destroyed, or no end in the rounds of a rollout. */
  draw: number | null;
  loss: number | null;
  /** How many rollouts the numbers are of. 0 before the first answer. */
  rollouts: number;
  /** The numbers are of other staged units: the answer for these is on its way. */
  pending: boolean;
  /** The attacker, then the defender: the units by kind, in the order of the units. */
  sides: {
    seat: SeatId;
    units: {
      unit: UnitType;
      /** The printed name of the unit ("Fighter II"). Without it the unit is named by its kind. */
      name?: string;
      /** The units that are expected to survive, with one decimal ("1.8"). Null before the first answer. */
      expected: string | null;
      /** The units that fight. */
      fielded: number;
    }[];
  }[];
}

/** The odds of the space combat that a staged movement starts. */
export type SpaceOddsView = CombatOddsView;

export interface BattleRecordView {
  key: string;
  label: string;
  table: BattleTableView;
}

// ---- Steps of a tactical action -----------------------------------------------------------------

export interface ActivationView {
  kind: "activation";
  editing: boolean;
  /** Editing: the system in the selection, if any. */
  chosen: {
    system: SystemId;
    label: string;
    facts: string;
    commandToken: boolean;
    shipsInRange: number;
    originsInRange: number;
    /** The production value of the player's units in the system. */
    production: number;
    /** The player's units that roll SPACE CANNON at the ships of another player in the system. */
    cannon: number;
  } | null;
  systems: { id: SystemId; label: string }[];
  /** Recorded: the active system. */
  active: { system: SystemId; name: string } | null;
  /** The tactic pool of the acting player, before and after the token of this activation. */
  tacticPool: [now: number, after: number];
  /** The activation places a command token. False: a tactical action without one (Warfare). */
  costsToken: boolean;
  draft: boolean;
}

/**
 * One ship. The game moves the ships one at a time and asks for each: a ship has its own path,
 * its own hold, and its own roll when it leaves a gravity rift.
 */
export interface ShipUnitView {
  /** The id of the staged value: 1 moves the ship, 0 keeps it. */
  key: string;
  /** "Carrier 2 of 4". */
  label: string;
  moves: boolean;
  /** False when the ship cannot move, or cannot move now. `reason` says why. */
  canMove: boolean;
  reason: string | null;
  invalid: boolean;
  /**
   * The move value of this ship, when the heading of its kind does not say it: "Move 1 → 2"
   * with Gravity Drive. Null when the heading says it.
   */
  move: string | null;
  /** The path that the game takes: "via #41 Gravity rift". Empty when the ship cannot move. */
  route: string;
  /** The ship leaves a gravity rift: one die decides whether it arrives, with its hold. */
  riftRoll: boolean;
  /**
   * Gravity Drive, where it changes what this ship does. One ship of the action has it.
   * `locked`: the ship cannot move without it. `reason`: why it cannot be switched on.
   */
  boost: { on: boolean; locked: boolean; reason: string | null } | null;
  /** Null for a ship without capacity, and for a ship that stays. */
  hold: ShipHoldView | null;
  link: LinkToken[];
}

/** The hold of one ship that moves. */
export interface ShipHoldView {
  capacity: number;
  loaded: number;
  /**
   * One for each loaded unit. `value` is the count that takes this unit out again.
   * `site` names the system on the way where the unit is picked up. `place` is where the unit
   * is in its system: "Planet Jord", "Space area".
   */
  slots: {
    key: string;
    unit: UnitType;
    name: string;
    value: number;
    site: string | null;
    place: string;
  }[];
  /** What this ship can load: the id of a cargo source, and the staged value that takes one more. */
  accepts: Record<string, { key: string; value: number }>;
}

/** One kind of ship in one system: what the ships have in common, and each ship. */
export interface MoveShipView {
  /** The ship line. */
  key: string;
  unit: UnitType;
  name: string;
  damaged: boolean;
  /** How many move, and how many are in the system. */
  count: number;
  total: number;
  /** "Move 1 · Capacity 4". */
  facts: Rich;
  /** Why no ship of this kind can move. */
  reason: string | null;
  invalid: boolean;
  link: LinkToken[];
  /** Every ship of the kind while the movement is open; in a read-only view those that moved. */
  units: ShipUnitView[];
}

/** Units that the ships of one system can load: where they start, or on their way. */
export interface CargoSourceView {
  id: string;
  unit: UnitType;
  name: string;
  /** Where the units are: "Planet Jord", "Space area". */
  place: string;
  /** A system on the way of a ship, where it picks the units up. Null for the system itself. */
  site: { system: SystemId; label: string } | null;
  /** How many are there to load. */
  left: number;
  /** Why the units cannot be loaded. */
  reason: string | null;
}

/** The ships that can leave one system, and what they load. */
export interface OriginView {
  system: SystemId;
  label: string;
  away: number;
  ships: MoveShipView[];
  /** What the ships can load. Empty in a read-only view. */
  cargo: CargoSourceView[];
  /** What leaves: "2 ships · 3 cargo". Empty when nothing is staged. */
  leaves: string;
  loaded: number;
  capacity: number;
  /** What stays in the system after the staged ships left. */
  stays: string;
  /** Units that use capacity and that no ship can load: they stay, with the reason. */
  warning: string | null;
  /** The player is done with this system: something leaves it, or the player marked it. */
  handled: boolean;
  /** What is in the system, as the inspector shows it. Only the sheet on the board has it. */
  details?: SystemDetailsView;
}

export interface RiftRowView {
  /** Null when the game does not name the ship that rolled. */
  unit: UnitType | null;
  name: string;
  text: string;
  /** Null before the roll. */
  roll: { face: number; lost: boolean } | null;
}

export interface MovementView {
  kind: "movement";
  editing: boolean;
  /** The active system. */
  target: { system: SystemId; label: string };
  origins: OriginView[];
  /** Ships and cargo that cannot move to the active system, each with the reason. */
  unreachable: {
    target: SystemId;
    rows: { unit: UnitType; name: string; text: string; system: SystemId }[];
  } | null;
  /** The fleet of the player in the active system, before and after the staged movement. */
  arrival: { unit: UnitType; name?: string; now: number; after: number }[];
  gauges: GaugeView[];
  excessShips: number;
  riftRolls: number;
  rift: RiftRowView[] | null;
  /** The names of the units that a rift destroyed: "Carrier II", "Infantry I". */
  removed: string[];
  /** What the space cannon offense rolled, else the guns that will roll, without dice. */
  cannon: BattleTableView | null;
  /** The space combat that the staged ships start. Null when no other seat has ships there, or no ship moves. */
  odds: SpaceOddsView | null;
}

export interface CombatView {
  kind: "combat";
  /**
   * The open question of the viewer that has no shape on the rows yet, as the list of a
   * decision: its prompt and its blocks. Null when the viewer is not asked, or when the question
   * is a reaction window (the band on top of the panel).
   */
  decision?: { prompt: string; blocks: BlockView[] } | null;
  records: BattleRecordView[];
  /** Changes when the battle moves on; the view then shows the latest record again. */
  revision: string;
}

/**
 * The odds of the ground combat that the staged landings start on a planet. The numbers are of
 * the last answer; `preview` is what the first round rolls with the units of that answer.
 */
export interface GroundOddsView extends CombatOddsView {
  /** The planet has a defense that the odds cannot count: there is no number. */
  unavailable: boolean;
  /** The table of the first round, with the odds in its headers. Null before the first answer. */
  preview: { table: BattleTableView; odds: OddsView } | null;
}

/** What the ground odds count and leave out: the end of the hint of the odds. */
export const GROUND_ODDS_COUNT =
  "Counted: the ground forces of both sides on the planet and those that land, the damaged ones, the SPACE CANNON of the defender against the landing, and the ships that fire during the combat (Harrow). Left out: action cards, promissory notes and leaders.";

/** One planet of the active system while the landings are staged. */
export interface LandingPlanetView {
  id: PlanetId;
  name: string;
  resources: number;
  influence: number;
  owner: SeatId | null;
  traits: PlanetTraitsView;
  /** The units that are on the planet now, by seat. */
  there: { seat: SeatId; force: Force }[];
  /** One for each staged unit. `value` is the count that takes this unit back. */
  tokens: { key: string; unit: UnitType; name: string; value: number }[];
  /** What a tap on the free slot lands: the id of a force, and the staged value with one more. */
  accepts: Record<string, { key: string; value: number }>;
  /** What lands: "2 Infantry, 1 Mech". Empty when nothing is staged. */
  lands: string;
  /** Why nothing can land here. */
  reason: string | null;
  /** The odds of the ground combat that the staged units start here. Null: no ground combat. */
  odds: GroundOddsView | null;
}

/**
 * The landings of an invasion, staged at the active system: the ground forces that can land,
 * and each planet with what lands there. The sheet on the board has the controls; the panel
 * says what lands.
 */
export interface LandingView {
  system: SystemId;
  label: string;
  /** The ground forces that can land, by kind and place, with the count that is left. */
  forces: CargoSourceView[];
  /** The force that a tap lands. Null when nothing is left. */
  chosen: string | null;
  planets: LandingPlanetView[];
  /** How many units land. */
  landed: number;
  /** What does not land: "1 Infantry". Empty when everything lands. */
  stays: string;
}

export interface InvasionView {
  kind: "invasion";
  /** The landings, staged at the active system. */
  landing: LandingView;
}

/** The total of a payment. The player chooses the planets on the board, not in the panel. */
export interface PaymentView {
  title: string;
  cost: number;
  paid: number;
  unit: string;
  /** False when the payment is recorded and read-only. */
  editable: boolean;
  /** What pays now, for example "Jord 4 + Lodor 1". Empty when nothing is chosen. */
  summary: string;
  /**
   * Trade goods are not on the board, so they have a counter in the panel. `rest` is the count
   * that pays exactly what the planets leave; null when the trade goods cannot do that.
   */
  goods: { id: string; value: number; max: number; rest: number | null } | null;
  /** Commodities that pay as trade goods (the Keleres agent): a counter next to the trade goods. */
  commodities?: { id: string; value: number; max: number } | null;
  /** Auto-pay and "Clear payment". They change the staged payment on the map; nothing is sent. */
  actions: ActionButtonView[];
}

/** The staged purchases of one unit in one place, with their counter. */
export interface BuildCountView {
  /** "Space area", "Planet Jord". */
  place: string;
  /** Counts purchases: `id` goes back in a `setCount` intent. Fighters come two for a purchase. */
  counter: CounterView;
  /** The units that the staged purchases make here. */
  made: number;
  /** Why no more of it can be staged. */
  reason: string | null;
}

/** One kind of unit of a production, as printed. */
export interface BuildUnitView {
  id: string;
  unit: UnitType;
  name: string;
  /** What one purchase costs: "2", or "1 for 2". */
  price: string;
  /** "9+", "3+ (x3)". "–" when the unit does not fight. */
  combat: string;
  /** "–" when the unit does not move by itself. */
  move: string;
  /** "–" when the unit carries nothing. */
  capacity: string;
  /** How many are left in the supply after what is staged. Null when there is no limit. */
  left: number | null;
  /** The keyword abilities, as printed: "SUSTAIN DAMAGE", "BOMBARDMENT 5+". */
  abilities: string[];
  /** The special rule that is printed on the unit. Empty when it has none. */
  rules: string;
  /**
   * Why the unit cannot be produced in this use at all: a word for its row ("None left"), and
   * the reason in full. The row has no counter.
   */
  unavailable: { word: string; reason: string } | null;
  /** One for each place that takes the unit. */
  counts: BuildCountView[];
}

/** The units of one kind of place: the ships, the ground forces. */
export interface BuildGroupView {
  /** "Ships · Space area", "Ground forces". */
  title: string;
  /** The units have a line for each place. Else the place is in the title. */
  byPlace: boolean;
  units: BuildUnitView[];
}

/** One place of the active system that takes produced units. */
export interface BuildPlaceView {
  id: string;
  /** "Space area", "Planet Jord". */
  name: string;
  /** What is produced here: "2 Fighters, 1 Cruiser". Empty when nothing is staged. */
  builds: string;
}

/** The production while it is staged: the sheet at the active system and the summary of the panel. */
export interface BuildSheetView {
  system: SystemId;
  label: string;
  groups: BuildGroupView[];
  places: BuildPlaceView[];
  /** How many units are staged. */
  made: number;
  /** What they cost, after a discount. */
  cost: number;
  /** What the cost of this use is reduced by. It is already taken off `cost`. */
  discount: number;
  /** The resources that the seat does not have for the staged build. 0 when it can pay. */
  short: number;
  gauges: GaugeView[];
  /** The build is chosen first; then the map is the payment. */
  stage: "build" | "pay";
  /** What is in the system, as the inspector shows it. Only the sheet on the board has it. */
  details?: SystemDetailsView;
}

export interface ProductionView {
  kind: "production";
  /** The production, staged on the map. */
  sheet: BuildSheetView;
  /** The payment of the build. The footer has its line, and the board its controls. */
  payment: PaymentView;
}

export type StepContentView =
  | ActivationView
  | MovementView
  | CombatView
  | InvasionView
  | ProductionView;

/** The parts of a step, in its heading. */
export type TrailView = { label: string; status: TrailStatus; reason?: string }[];

/** An entry of a draft that no longer fits: the row that marks it in its step. */
export interface ReviewView {
  /** The unit that the entry names. */
  icon: UnitType | null;
  /** What the entry is: "Carrier from Jord · #01". */
  title: string;
  /** Why it no longer fits. */
  reason: string;
  /** Takes the entry out. Null when only a change of the step fixes it. */
  remove: ActionButtonView | null;
}

export interface TaskView {
  step: number;
  title: string;
  /** The state of the step; null when the content says it already (the stage of a watched round). */
  pill: PillView | null;
  trail: TrailView;
  edit: { label: string; step: number; disabled: boolean; hint?: string } | null;
  /** The entry of a draft that no longer fits, when it is of this step. */
  review?: ReviewView | null;
  /** Rules and explanations of this step. */
  help: HelpView[];
  /** Whom the content of the step serves: a combat that the viewer watches informs. */
  role: BlockRole;
  content: StepContentView;
  footer: FooterView;
}

// ---- Blocks for actions without a dedicated view: strategy cards, component actions ------------

export type RowControlView =
  | { kind: "button"; button: ActionButtonView }
  | { kind: "counter"; counter: CounterView }
  | { kind: "badge"; pill: PillView };

export interface RowView {
  icon?: "planet" | "card" | "token" | "minus";
  order?: number;
  title: Rich;
  subtitle?: string;
  /** A pool of command tokens: how many it has now, and how many after the staged change. */
  tokens?: { now: number; after: number; /** The fleet pool: its tokens point up. */ up?: boolean };
  /** Rule text, shown on hover or click. */
  hint?: string;
  link?: LinkToken[];
  control?: RowControlView;
}

/** One choice of a menu. The whole row is the control. */
export interface MenuRowView {
  /** Shown on the first row of a group only. */
  group?: string;
  /** Shortcut key: a letter or a digit. Empty for a row that has none: a list with more rows than keys. */
  key: string;
  title: string;
  /** Game state of this choice: a count, or the reason why it is not allowed. */
  state?: string;
  /** Rule text, shown on hover or click. */
  hint?: string;
  /** Printed text in titled parts, shown on hover or click: the primary and the secondary of a strategy card. */
  help?: HelpView[];
  /** The printed text of a card, shown under the title. For a list that has the room for it. */
  text?: string;
  /** The staged choice of a list where the player chooses one row. */
  selected?: boolean;
  /** Of a row of a reaction window: when the viewer is offered this reaction. */
  offer?: OfferView;
  disabled: boolean;
  intent: Intent;
}

/** One seat of a strategy card. Only the row of the viewer can be open. */
export interface SeatRowView {
  /** "P" for the primary, then the place in the seat order. */
  order: string;
  seat: SeatId;
  name: string;
  you: boolean;
  status: { tone: PillView["tone"]; sign: string; label: string };
  /** What the seat did, or what its draft will do. */
  text?: string;
  /** The editor of the viewer, in place under the row. */
  open?: BlockView[];
}

/** One technology of the tree. The cell is the control. */
export interface TechCellView {
  id: string;
  name: string;
  /** The prerequisites, one for each symbol. `missing` is the reason why it cannot be researched. */
  needs: { color: TechColor; missing: boolean }[];
  /** The colour of the technology, for a cell that is not in a colour column. */
  color?: TechColor;
  state: "owned" | "open" | "closed";
  selected: boolean;
  /** The printed text, shown on hover. */
  text: string;
  intent: Intent;
}

export interface TechColumnView {
  color: TechColor;
  /** What the viewer has of this colour: "Blue 2 · +1 Arnor". */
  label: string;
  cells: TechCellView[];
}

/** One thing that a side of a deal hands over: an amount, or a card. */
export interface DealItemView {
  /** "Trade goods", "Ceasefire (Hacan)". */
  name: string;
  /** The group of a card, named once for its cards: "Promissory notes", "Action cards". */
  group?: string;
  /** The printed text of a card, shown on hover or click. */
  help?: HelpView;
  /** An amount that is staged. */
  counter?: CounterView;
  /** An amount of a deal that is read-only. */
  count?: number;
  /** A card that is staged with a tap. Absent in a deal that is read-only. */
  toggle?: { on: boolean; intent: Intent };
}

/** One side of a deal: the seat, what it holds now, and what it hands over. */
export interface DealSideView {
  /** "You give", "You get". */
  title: string;
  seat: SeatId;
  name: string;
  /** "Trade goods 3 · Commodities 2". */
  holds: string;
  items: DealItemView[];
}

export type BlockView = (
  | { kind: "note"; tone: "accent" | "success" | "quiet"; strong?: string; text: string }
  | { kind: "menu"; title?: string; rows: MenuRowView[] }
  /**
   * The card or ability that asks for the decision, as printed. It is on top of the decision.
   * `choices` are a choice of one about this card, a segmented toggle on the line of its name
   * (the agenda deck of Politics: "Top", "Bottom"); they have keys as the rows of a menu do.
   */
  | { kind: "source"; name: string; type: string; text: string; choices?: MenuRowView[] }
  | { kind: "gauges"; gauges: GaugeView[] }
  /** The technology tree: one column for each colour, and a band for the unit upgrades. */
  | {
      kind: "techs";
      title: string;
      /** The chosen technologies with their price. Empty when nothing is chosen. */
      chosen: string[];
      aside?: string;
      columns: TechColumnView[];
      /** Rows under the columns: the unit upgrades, and the faction technologies if there are any. */
      bands: { label: string; cells: TechCellView[] }[];
    }
  /** The seat order of a strategy card: the primary, then the secondaries, with what each did. */
  | { kind: "seats"; rows: SeatRowView[] }
  | {
      kind: "card";
      title: string;
      aside?: string;
      bad?: boolean;
      rows: RowView[];
    }
  /** A transaction: what the viewer gives, and what the viewer gets, side by side. */
  | { kind: "deal"; sides: DealSideView[] }
  | {
      kind: "draft";
      title: string;
      pill: PillView;
      live: boolean;
      text?: string;
      hint?: string;
      blocks: BlockView[];
    }
) & {
  /** Whom the block serves. Absent: it decides. */
  role?: BlockRole;
};

/**
 * A window on top of the open action: a reaction window (the viewer may play a card now, or pass)
 * or an ability that asks the viewer in the middle of another action (Scheming in a Politics).
 * The action under it shows what happened. The main button of the footer is Pass or Play, or
 * the answer of the ability. On a phone it is a sheet on the map too, where its system is marked.
 */
export interface InterruptView {
  /**
   * What opened it, in game values: "Reaction · After Blair plays Mining Initiative". The name
   * of the band; the heading and its context say it on the screen.
   */
  label: string;
  /**
   * The ability that asks, as printed, in one line: its name, its kind, its text behind "?".
   * Null for a reaction window, whose rows are its cards.
   */
  source: { name: string; type: string; text: string } | null;
  /** What the ability asks, without its name: "Choose and discard 1 action card from your hand." */
  question: string | null;
  link: LinkToken[];
  blocks: BlockView[];
}

// ---- The panel ----------------------------------------------------------------------------------

interface PanelFrame {
  /**
   * What the panel is about that the heading does not say, muted beside it: the kind of action, its
   * system and its seat ("Tactical action · #26 · Hacan"), or "Turn ended". Null when the heading says it all.
   */
  context: string | null;
  /** "Not sent" while a choice is staged, beside the pill of the heading. Null in any other state. */
  badge: PillView | null;
  tabs: StepTabView[];
  selected: number;
  current: number | null;
  /** Key of the content, to keep scroll position while the same content is open. */
  contentKey: string;
  /** A reaction window of the viewer, on top of the content. Null in any other state. */
  interrupt: InterruptView | null;
  /** What the viewer can still do after their action, before they end the turn. Empty in any other state. */
  closing: BlockView[];
}

export interface TacticalActionView extends PanelFrame {
  kind: "tactical";
  task: TaskView;
}

export interface FlowActionView extends PanelFrame {
  kind: "flow";
  heading: string;
  pill: PillView | null;
  trail: { label: string; status: TrailStatus }[];
  /** Rules and explanations of this action. */
  help: HelpView[];
  blocks: BlockView[];
  footer: FooterView;
}

export type ActionView = TacticalActionView | FlowActionView;

export type { Tone };
