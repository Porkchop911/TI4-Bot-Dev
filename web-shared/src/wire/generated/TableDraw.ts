// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { FactionId } from "./FactionId.ts";
import type { PlayerId } from "./PlayerId.ts";

/**
 * What Start draws for a table, with the ids of this host.
 */
export type TableDraw = { 
/**
 * The players in the order of their seats: the player of seat `i` is [`SEATS`]`[i]`.
 */
players: Array<PlayerId>, 
/**
 * The faction of each seat. Empty for a table that starts from a scenario: the scenario
 * seats its own factions.
 */
factions: Array<FactionId>, };
