// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { SeatRequest } from "./SeatRequest.ts";
import type { TableSetup } from "./TableSetup.ts";

/**
 * The table of web2's single player: what the host chose (`ti4_host::setup::TableSetup`), and
 * its players in the order of their seats. The player of seat `i` has the id of [`SEATS`]`[i]`:
 * the masks of this host name a player by that letter.
 */
export type TableRequest = { setup: TableSetup, 
/**
 * Missing entries are seats of the page with no chosen faction.
 */
seats: Array<SeatRequest>, };
