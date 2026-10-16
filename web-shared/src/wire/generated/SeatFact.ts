// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * Who sits in a seat of a running game. It is public: a host tells it to every viewer with the
 * game, so a page names a seat by its player.
 */
export type SeatFact = { 
/**
 * The name of the player at the table.
 */
nickname: string, 
/**
 * Who plays the seat: `human`, or a kind of `ti4_host::setup::bot_kind`.
 */
kind: string, };
