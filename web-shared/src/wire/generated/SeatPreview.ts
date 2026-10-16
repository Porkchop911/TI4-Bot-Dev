// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * The place of one seat on the previewed map. It names no faction: nobody knows before Start
 * who sits where, and which faction a seat without a choice gets.
 */
export type SeatPreview = { 
/**
 * 1-based number of the place, in the seat order of the map.
 */
seat: number, 
/**
 * The id of the tile of [`MapPreview::tiles`] that stands for the home of this seat
 * (`seat_1`, …). It is not a system of the game.
 */
home_system_id: string, };
