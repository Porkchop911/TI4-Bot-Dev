// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * How the engine would pay a Technology secondary's resources: the first plan of
 * [`crate::payment::plans`], which is the one `paid_research` applies.
 */
export type PaymentPreview = { 
/**
 * Resources owed.
 */
cost: number, 
/**
 * Planets that would be exhausted.
 */
planets: Array<string>, 
/**
 * Trade goods that would be spent.
 */
trade_goods: number, 
/**
 * What the plan is worth (at least `cost`).
 */
worth: number, };
