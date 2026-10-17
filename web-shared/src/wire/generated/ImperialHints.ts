// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * The look-ahead of Imperial's questions, the question of the primary's public objective and
 * the window of its secondary: its fields are top-level keys of the details,
 * `details.secrets_held` and `details.secrets_max`, written with [`Choice::hinted_fields`], so
 * that a client can say what drawing a secret objective after it does (display only).
 */
export type ImperialHints = { 
/**
 * The secret objectives that the seat holds now, scored ones included.
 */
secrets_held: number, 
/**
 * How many it may hold ([`secret_limit`]).
 */
secrets_max: number, };
