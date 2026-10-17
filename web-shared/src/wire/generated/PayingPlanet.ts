// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * A planet that can pay resources, with what it is worth to the seat.
 */
export type PayingPlanet = { planet: string, worth: number, 
/**
 * The kind of value that the planet pays with, when it is not the kind of the bill: its
 * influence for resources (Archon's Gift, Freelancers). The payment option of the planet is
 * then `exhaust|<planet>|<face>`.
 */
face?: string, };
