import type {
  BlockView,
  BuildSheetView,
  LandingView,
  OriginView,
  PaymentView,
  SpaceOddsView,
} from "./action";
import type { Force, PlanetId, SeatId, SystemId } from "./core";

export type Anomaly = "asteroid" | "supernova" | "nebula" | "rift";
export type TechColor = "B" | "G" | "Y" | "R";

/** The type of a planet, which is also its exploration deck. */
export type PlanetType = "cultural" | "hazardous" | "industrial";
/** An exploration deck: one for each planet type, and the frontier deck. */
export type ExplorationDeck = PlanetType | "frontier";

/** What is attached to a planet. `effect` is short ("+2 / +1"); `text` is the printed text. */
export interface AttachmentView {
  name: string;
  effect: string;
  text: string;
}

/** What the exploration says about one planet. The same line on every screen that has a planet. */
export interface PlanetTraitsView {
  /** More than one type: the player who explores the planet chooses the deck. */
  types: PlanetType[];
  /** A player who takes control of the planet explores it. */
  unexplored: boolean;
  attachments: AttachmentView[];
}

/** One planet on a tile. Compare `PlanetMetaView` + `PlanetView`. */
export interface PlanetMarkView {
  id: PlanetId;
  name: string;
  resources: number;
  influence: number;
  owner: SeatId | null;
  tech: TechColor | null;
  groundForces: number;
  planetaryShield: boolean;
  traits: PlanetTraitsView;
  /** Every map view draws the outline of an exhausted planet dashed. */
  exhausted: boolean;
}

/** One system on the board. Compare `BoardTileView` + `SystemView`. */
export interface TileView {
  id: SystemId;
  /** Not drawn on the board. The inspector and assistive technology use it. */
  name: string;
  q: number;
  r: number;
  home: SeatId | null;
  anomaly: Anomaly | null;
  wormhole: string | null;
  /** The viewer's command token is here. */
  commandToken: boolean;
  /** At most three. */
  planets: PlanetMarkView[];
  /** Who has ships here: one seat, or "contested" when two or more players have ships. */
  control: SeatId | "contested" | null;
  /**
   * Ships in the space area. `strength` is the average number of hits per combat round.
   * `was` is the count before the staged movement, when that changes it: the mark shows "3 → 1".
   */
  fleets: { seat: SeatId; ships: number; strength: number; was?: number }[];
  /** Cargo that staged ships pick up here. */
  pickedUp: number;
  /**
   * What the open movement means for this system and has no number: ships that cannot leave,
   * cargo that no ship takes. A sign on the tile; the text is in the tooltip and the inspector.
   */
  note?: { sign: "stay" | "cargo"; text: string };
  /**
   * The player is done with this system in the open movement: ships leave, a ship picks up
   * units, or the player marked it.
   */
  handled?: boolean;
}

export interface RouteView {
  /** Link token id: a row with `rt:<id>` lights this route. */
  id: string;
  /** The rows that light this route, when it stands for more than one. */
  links?: string[];
  path: SystemId[];
  staged: boolean;
  /** What happens on the way, at the system where it happens. */
  marks?: { system: SystemId; sign: "die" | "plus"; text: string }[];
}

/**
 * What the open task lets the player choose on the board: planets or systems.
 * It is drawn over any map view. The keys of `values` are the things that can be chosen.
 */
export interface BoardTaskView {
  target: "planet" | "system";
  kind: "pay" | "pick";
  /** False when the choices are only shown, for example a recorded payment. */
  interactive: boolean;
  values: Record<PlanetId | SystemId, number>;
  chosen: Record<PlanetId | SystemId, boolean>;
  verb: string;
  unit: string;
  /**
   * The systems of `values` where the choice does something. When it is set, the other systems
   * of `values` can be chosen and do not stand out.
   */
  highlight?: SystemId[];
  /** Systems that cannot be chosen and belong to the choice: they are not dimmed. */
  context?: SystemId[];
  /**
   * A task on planets that chooses systems too (the system of Construction): a click on one of
   * these systems chooses it (`toggleSystem`), as in a task on systems; `chosen` marks it.
   */
  systems?: SystemId[];
  /** A payment that is open: the board has its state and its controls, where the player chooses. */
  payment?: PaymentView;
  /** The planets that pay with their other value (Archon's Gift): the kind that they pay with. */
  faces?: Record<PlanetId, string>;
}

export interface InspectorRowView {
  /** A seat row shows a fleet; a planet row shows a planet. */
  seat?: SeatId;
  planet?: {
    name: string;
    resources: number;
    influence: number;
    owner: SeatId | null;
    traits: PlanetTraitsView;
  };
  label?: string;
  text?: string;
  force?: Force;
  notes?: { text: string; tone: "muted" | "planned" }[];
}

/** What is in one system. The inspector shows it, and every sheet of a task at a system. */
export interface SystemDetailsView {
  facts: string;
  notes: string[];
  rows: InspectorRowView[];
}

export interface InspectorView extends SystemDetailsView {
  system: SystemId;
  title: string;
  /** Set when the player may start a tactical action here. */
  activate: "allowed" | "token" | null;
  /**
   * The active system while the movement is staged: the ships that are committed to it, by the
   * system that they leave, each with what it carries. Read-only; the controls are at those systems.
   */
  arriving?: { summary: string; origins: OriginView[]; odds: SpaceOddsView | null };
}

export interface BoardView {
  tiles: TileView[];
  wormholes: [SystemId, SystemId][];
  routes: RouteView[];
  activeSystem: SystemId | null;
  inspected: SystemId | null;
  /** A click on a system chooses the activation target. */
  targeting: boolean;
  task: BoardTaskView | null;
  inspector: InspectorView | null;
  /** The fleet of the inspected system, with its controls, while the movement is staged on the board. */
  origin: OriginView | null;
  /** The landings of the active system, with their controls, while they are staged and open. */
  landing: LandingView | null;
  /** The build of the active system, with its controls, while it is staged and open. */
  production: BuildSheetView | null;
  /**
   * The blocks of an open decision whose payment is on the map (the command pools of
   * Leadership): a phone shows them on the map with the payment, so that the whole choice is in
   * one pane. Desktop has them in the action panel next to the map.
   */
  blocks?: BlockView[];
  /** The systems of the open task, for "Fit task". */
  taskSystems: SystemId[];
  /** Changes when the board must be framed again, for example for a new action. */
  fitKey: string;
}
