// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { MapChoice } from "./MapChoice.ts";
import type { SpeakerOrder } from "./SpeakerOrder.ts";

/**
 * What a host chose for a table. The players and their factions are not a part of it: they are
 * given to [`draw`]. Both hosts start a table from it, so a table of a seed is the same game in
 * the page and on the server.
 */
export type TableSetup = { player_count: number, 
/**
 * The map of the table; a table that names none plays the seeded random board.
 */
map?: MapChoice, 
/**
 * One of the sets of the content (a table offers [`card_set::OFFERED`]).
 */
strategy_cards?: string, 
/**
 * Left out: `random`, drawn from the seed. `seat` is the first seat.
 */
speaker_order?: SpeakerOrder, 
/**
 * The id of a scenario of [`crate::view::scenario`]: the table starts that game, with its seed,
 * its seats, its map, its cards and its factions. `None` is round 1.
 */
start_from?: string, 
/**
 * A start preset of [`crate::preset`] (a test table): it seats its own factions and sets its
 * cases on the opening state. A scenario wins over it.
 */
preset?: string, 
/**
 * The seed of the table. It tells the factions that nobody chose, so a host
 * with several people keeps it to itself.
 */
seed: number, };
