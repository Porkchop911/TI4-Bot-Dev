// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * A card that is a name and a text: a technology, a relic, the ability card of a legendary
 * planet.
 */
export type PrintedCard = { id: string, name: string, text: string, 
/**
 * The card is used: its text has an `ACTION`, or it exhausts. A card that always holds is
 * not.
 */
used: boolean, };
