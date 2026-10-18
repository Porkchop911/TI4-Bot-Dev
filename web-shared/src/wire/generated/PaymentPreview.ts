// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * The cheapest way to pay a Technology secondary's resources: the first plan of
 * [`crate::payment::plans`]. The player chooses the payment when the game asks for it
 * (`pay_resources`, one planet or trade good per question); a lone way is taken without asking.
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
