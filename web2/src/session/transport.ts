import type { MovementPlan, PlanStep } from "./movementPlan";
import type { SeatStore } from "./seatStore";
import type {
  BattleResponse,
  Catalog,
  DraftOutcome,
  DraftStep,
  PlanScope,
  PublicTurnStatus,
  ReactionMode,
  ReactionModes,
  SessionUpdate,
  SpaceOddsRequest,
} from "./wire";

/** A plan that did not reach its end. Compare `BatchInterruption` and `BatchFailure` of the server. */
export interface PlanStopped {
  /** The active system of the plan. */
  destination: string;
  /** How many steps are in the game. 0 when the plan was refused: the game is as before. */
  applied: number;
  /** The steps that are not in the game. */
  remaining: PlanStep[];
  /** True when the game asked something else first; false when it refused a step. */
  interrupted: boolean;
  reason: string;
}

/** What a transport tells its listeners: the game as the viewer is shown it, or why it stopped. */
export type TransportEvent =
  | {
      kind: "update";
      update: SessionUpdate;
      /**
       * This seat may take back its last answer. In a local game that is any answer of the seats
       * that are played here; on the server, an answer after which nothing hidden was shown,
       * nothing was rolled and no other seat decided.
       */
      canUndo: boolean;
      /** The plan that was sent before this update stopped here. */
      plan?: PlanStopped;
      /** The steps of the movement or of the landings that the undo before this update took back, in order. */
      undone?: PlanStep[];
      /** The game waits before a decision of this seat, which is not played here: `capabilities.stepping.step` goes on. */
      stepping?: { seat: string };
    }
  /**
   * Who the game waits for changed, and nothing else of the game: a seat that is not played here
   * takes its time. What is staged stays.
   */
  | { kind: "status"; status: PublicTurnStatus }
  /** The game is played again up to where it was. The last update is out of date until the next. */
  | { kind: "replaying"; done: number; total: number }
  /** The game cannot go on. The text names the cause. */
  | { kind: "error"; message: string }
  /**
   * The answer to `draft(script, scope)`, with the script and the scope that were asked: a
   * listener drops the answer for a script that is no longer its newest. Null while the game
   * does not wait for this side.
   */
  | { kind: "draft"; script: DraftStep[]; scope?: PlanScope; outcome: DraftOutcome | null }
  /** The mode of each reaction that this side set, by its key, after each change. */
  | { kind: "modes"; modes: ReactionModes }
  /** A window was declined for this side, because this side muted everything that it offered: these, by name. */
  | { kind: "passed"; cards: string[] }
  /**
   * The game cannot be reached, and the transport tries again. The last update stays in view and
   * nothing can be sent; the next update ends it. A game in this page is never offline.
   */
  | { kind: "offline" }
  /**
   * The game did not take what this side sent, and is as it was: another answer was there
   * first, or the choice is no longer open. The update that follows is the game as it is.
   */
  | { kind: "rejected"; reason: string };

/**
 * What a game can do besides play. Each is a tool for testing a local game, and absent from a
 * game on the server. The page shows a control when its capability is there, never by the kind
 * of the transport.
 */
export interface Capabilities {
  /** The game can wait before each decision of a seat that is not played here. */
  stepping?: {
    /** The seats decide the same either way. */
    set(on: boolean): void;
    /** Lets the seat decide that the game waits for (`stepping` of the last update). */
    step(): void;
  };
  /**
   * Plays this seat here from now on, in place of the seat that the game is shown as: that seat
   * decides at random from its open decision on. Does nothing while the game does not wait for
   * this side, or when the seat is played here already.
   */
  switchSeat?(seat: string): void;
  /**
   * The wait of the seats that this page plays, in milliseconds. A seat that waits now finishes
   * its wait. It is of the table of the game: the sheet "Table" has its control, and the app
   * that made the transport keeps it.
   */
  wait?(ms: number): void;
  /** The game can start from a prepared board. The app that made the transport has the list. */
  scenarios?: true;
  /** The game is kept in this page: the app that made the transport can start it again, export it and import it. */
  save?: true;
}

/**
 * Where the game runs: the wasm engine in this page (`wasmTransport.ts`), or the server behind
 * a websocket (`wsTransport.ts`), with `StateUpdateMsg` as the update and `SubmitChoice` as the
 * answer.
 */
export interface Transport {
  /** The printed cards. An update names them by id only. */
  readonly catalog: Catalog | null;
  readonly capabilities: Capabilities;
  /**
   * Where the game keeps what this seat has staged and not sent: the server, the same in every
   * tab and device of the seat, or the engine of a local game, in its saved game. The game sends
   * a ready draft of a secondary itself. Absent for a spectator, who stages nothing.
   */
  readonly staged?: SeatStore;
  /**
   * A new listener is told the cards that are never offered and the latest update, replay or
   * error at once, if there is one.
   */
  subscribe(listener: (event: TransportEvent) => void): () => void;
  /** Answers the pending choice that has this nonce. Any other nonce is ignored. */
  submitChoice(nonce: string, optionId: string): void;
  /**
   * Answers the pending choice and the choices after it with the steps of a movement or of the
   * landings, as one request. A step that the game refuses takes the whole plan back. A question of the game that
   * is not of the movement stops the plan there. Either is told with the next update.
   */
  submitPlan(nonce: string, plan: MovementPlan): void;
  /**
   * Asks to play a draft script on a copy of the game: as the next turn of this side, or with
   * `scope` as its secondary of the strategic action that is resolved (`secondary` of the
   * update says when). The game is not changed. Where the run ended is told later, as a `draft`
   * event with this script and this scope. Only the newest script that waits is answered.
   */
  draft(script: DraftStep[], scope?: PlanScope): void;
  /**
   * Sends a draft to the game: the pending choice and the choices after it are answered with
   * the options of these ids, then with the steps of the movement, as one request with the
   * rules of `submitPlan`. The draft of a secondary is its answers by id, with no plan.
   */
  applyDraft(nonce: string, draft: { choices: string[]; plan: MovementPlan | null }): void;
  /**
   * When a window offers a reaction to this side: an action card by its printed name, the card
   * of an ability by its key (`ReactionOffer.key`). A window that offers only what this side
   * muted is declined where the game runs (a `passed` event), and any other window is asked
   * without it. The change is told as a `modes` event.
   */
  setReactionMode(key: string, mode: ReactionMode): void;
  /**
   * Takes back the last answer of this side, when `canUndo` of the last update says so; a
   * movement is taken back whole, and so are the landings, and the next update tells the steps.
   */
  undo(): void;
  /**
   * The odds of a space combat between two sides, from the engine of this page also online: the
   * same sides give the same numbers. Nothing of the game is read or changed.
   */
  odds(request: SpaceOddsRequest): Promise<BattleResponse>;
  /** Ends the game on this side. */
  close(): void;
}
