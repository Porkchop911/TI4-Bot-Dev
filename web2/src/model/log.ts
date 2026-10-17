import type { SeatId } from "./core";

/** The dice of one roll, as they showed. */
export interface LogDiceView {
  faces: number[];
  /** The face that a die needs to hit. Null for a roll that is no hit roll. */
  hitsOn: number | null;
  hits: number;
}

/** One line of the details of an entry: what one seat did, in words. */
export interface LogLineView {
  /** Whose line it is. Null for a line of no seat. */
  seat: SeatId | null;
  /** "Mecatol Rex · #18 → Abyz, Fria · #38: 2 Carrier I, Cruiser I". */
  text: string;
  /** The dice of a roll. */
  dice?: LogDiceView;
  /** A vote of the agenda phase: its outcome (null: abstained), its votes and the planets. */
  vote?: { outcome: string | null; votes: number; planets: string[] };
}

/** The details of an entry, in groups: "Movement", "Space combat", "Secondaries". */
export interface LogGroupView {
  title: string;
  lines: LogLineView[];
  /** The group of one agenda of the agenda phase. */
  agenda?: true;
}

/** The action that a turn took. "other" is a part of the turn menu that started no action (a transaction). */
export type LogAction = "tactical" | "strategic" | "component" | "pass" | "other";

/** One turn of the action phase: what one seat did with one action. */
export interface LogTurnView {
  kind: "turn";
  /** Stable for the turn: its `action_id`. */
  id: string;
  seat: SeatId;
  /** The turn is the viewer's. */
  mine: boolean;
  action: LogAction;
  /** "Tactical · Mecatol Rex · #18", "Strategic · Warfare", "Component · Sabotage", "Pass". */
  label: string;
  round: number;
  /** The turn of the action phase of its round, from 1. */
  turn: number;
  /** The most important outcomes in one line: "moved 3 ships · landed on Abyz, Fria". Empty: none. */
  summary: string;
  /** The details, in groups. Empty for a pass. */
  groups: LogGroupView[];
  /** The turn is played now: it is the last entry. */
  live: boolean;
}

/**
 * A phase of a round. A phase with no details (the action phase, whose turns follow it) is a
 * divider between the turns.
 */
export interface LogPhaseView {
  kind: "phase";
  /** Stable for the phase: "strategy-3". */
  id: string;
  phase: "strategy" | "action" | "status" | "agenda";
  round: number;
  /** "Strategy phase · Round 3". */
  label: string;
  /** "6 strategy cards picked", "2 objectives scored". Empty: none. */
  summary: string;
  /** What the seats did in it: the picks, the scored objectives, the votes of each agenda. */
  groups: LogGroupView[];
  /**
   * The phase is played now (a strategy, status or agenda phase that is the last entry): the
   * action that is played now is its content.
   */
  live: boolean;
}

export type LogItemView = LogTurnView | LogPhaseView;

/** The Log: what the seats did, oldest first, the entry that is played now last. */
export interface LogView {
  /**
   * `since-last-turn`: from the last turn of the viewer, which is the first entry. `round`: the
   * round in play, for a spectator and before the first turn of the viewer.
   */
  scope: "since-last-turn" | "round";
  entries: LogItemView[];
}
