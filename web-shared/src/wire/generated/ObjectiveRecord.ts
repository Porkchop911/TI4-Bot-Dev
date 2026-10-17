// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One secret or public objective as printed.
 */
export type ObjectiveRecord = { id: string, name: string, 
/**
 * The phase in which it is scored: `Status`, `Action` or `Agenda`. `Status` where the
 * corpus has none.
 */
phase: string, 
/**
 * What it is worth. 1 where the corpus has none.
 */
points: number, text: string, };
