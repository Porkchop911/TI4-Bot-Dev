// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One legal thing an actor may pick.
 *
 * `id` must be stable for a given game state so decision logs replay faithfully.
 */
export type ChoiceOption = { id: string, kind: string, label: string, 
/**
 * Structured detail for whatever will apply this option. Excluded from equality, as in
 * the oracle: two options with the same id are the same option, and a payload that
 * differed would mean the id was not stable.
 */
payload: Record<string, unknown>, 
/**
 * True when this is the only legal option and was auto-selected. For UX feedback only;
 * does not affect game logic or replay.
 */
auto_resolved?: boolean, };
