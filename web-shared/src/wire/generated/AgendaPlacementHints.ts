// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * The look-ahead of each agenda placement of Politics' primary: its field is a top-level key
 * of the details, `details.agendas`, written with [`Choice::hinted_fields`], so that a client
 * can stage the whole placement at once (display only).
 */
export type AgendaPlacementHints = { 
/**
 * The agenda of this question and the ones placed after it, in the order that they are
 * asked.
 */
agendas: Array<string>, };
