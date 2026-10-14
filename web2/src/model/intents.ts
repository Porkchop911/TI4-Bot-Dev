import type { ExplorationDeck } from "./board";
import type { OfferMode, PlanetId, SeatId, SystemId, UnitType } from "./core";

/**
 * Everything a player can ask for. The data source decides what an intent does: the mock engine
 * today, `submit_choice` and the planning messages of the wire protocol later.
 */
export type Intent =
  // Workspaces
  | { type: "workspace"; mode: "live" | "draft" | "history" }
  | { type: "closeHistory" }
  | { type: "undo" }
  | { type: "redo" }
  | { type: "startOver" }
  | { type: "discardDraft" }
  /** Asks for the confirmation dialog. The shell answers this one itself. */
  | { type: "openApply" }
  | { type: "confirmApply" }
  // Steps
  | { type: "selectStep"; step: number }
  | { type: "returnToCurrent" }
  | { type: "returnToEdit" }
  | { type: "editStep"; step: number }
  | { type: "cancelEdit" }
  | { type: "commitEdit" }
  // Staged values of the open step
  | { type: "setCount"; key: string; value: number }
  | { type: "removeLine"; key: string }
  /**
   * Loads one ship, or every ship that leaves a system ("origin:<system>"), until the holds are
   * full or nothing is left. `source` is the kind of unit that is loaded: a cargo source.
   */
  | { type: "fillHold"; key: string; source?: string }
  /** Gives Gravity Drive to one ship of the action, or takes it back. The path follows. */
  | { type: "setBoost"; ship: string | null }
  /**
   * Takes back everything that leaves with the ships of one system, its staged landings, or
   * its staged production.
   */
  | { type: "resetOrigin"; system: SystemId }
  /**
   * The player is done with one system of the movement: it gets its mark, and its fleet closes.
   * In an invasion it closes the landings of the active system. In a production it closes the
   * build: the payment is next.
   */
  | { type: "markOrigin"; system: SystemId }
  | { type: "setPlacement"; unit: UnitType; place: string }
  | { type: "setPayment"; source: string; value: number }
  /** Stages a suggested payment on the map, or clears the staged payment. */
  | { type: "payment"; action: "auto" | "reset" }
  // Board
  | { type: "inspectSystem"; system: SystemId }
  | { type: "closeInspector" }
  | { type: "findSystem"; query: string }
  | { type: "togglePlanet"; planet: PlanetId }
  | { type: "toggleSystem"; system: SystemId }
  // Battles
  | {
      type: "battle";
      action: "roll" | "announceRetreat" | "assignHits" | "resetHits" | "retreat";
    }
  | { type: "stageHit"; unit: UnitType; kind: "sustain" | "destroy" | "destroy-damaged" }
  // Other actions
  | {
      type: "pickAction";
      action: "tactical" | "strategic" | "component";
      system?: SystemId;
      /** The strategy card (its number) or the action card (its name). */
      card?: string;
    }
  /** The second step of a choice that has one row in the picker: which action card. */
  | { type: "pickGroup"; group: "actionCards" }
  | { type: "backToPicker" }
  | { type: "pass" }
  | { type: "endTurn" }
  | { type: "flow"; action: "resolve" | "ready" | "change" | "play" }
  | { type: "flowCount"; key: string; value: number }
  // Decisions: the staged choice. The main button of the footer sends it.
  | { type: "pickSeat"; seat: SeatId }
  /**
   * `group` names the list when a screen has more than one. "landing" is the kind of ground
   * force that a tap lands: a choice of the hand, which the game is not told.
   */
  | { type: "chooseOption"; option: string; group?: string }
  | { type: "pickTech"; tech: string }
  | { type: "clearChoice" }
  /** "Change" on a chosen thing: clears that part of the action, so it is open again. */
  | { type: "clearPart"; part: string }
  // Reaction windows: a card is staged, then played; or the viewer passes.
  | { type: "stageReaction"; card: string }
  | { type: "reaction"; action: "pass" | "play" }
  /**
   * When the viewer is offered a reaction: an action card by its printed name, the card of an
   * ability by its key. `name` is what the player is told.
   */
  | { type: "setOffer"; key: string; name: string; mode: OfferMode }
  /** The viewer acts as if holding this card, or stops. Nothing is sent. */
  | { type: "setCardFake"; card: string; fake: boolean }
  /** Opens a reference sheet. The shell answers this one itself. */
  | { type: "openReference"; sheet: "cards" }
  /** Opens the exploration decks at one deck. */
  | { type: "openReference"; sheet: "exploration"; deck: ExplorationDeck }
  | { type: "inspectLogEntry"; id: string }
  /** Lets another seat act. Only the demo offers this. */
  | { type: "simulate" };
