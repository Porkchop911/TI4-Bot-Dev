// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ExplorationRecord } from "./ExplorationRecord.ts";
import type { PlanetId } from "./PlanetId.ts";

/**
 * The exploration as every player at the table sees it.
 */
export type ExplorationFacts = { 
/**
 * The cards that each deck still holds, sorted by id. The order of a deck is hidden; what
 * left it was shown to the table.
 */
decks: { [key in string]: Array<string> }, 
/**
 * Every card that was drawn, in order.
 */
drawn: Array<ExplorationRecord>, 
/**
 * The planets that no player controls and that have a deck: a player who takes control of
 * one explores it. More than one deck is the choice of that player.
 */
explores: { [key in PlanetId]: Array<string> }, };
