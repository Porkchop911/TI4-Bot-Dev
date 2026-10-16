// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * Public cursor counts decisions, not engine steps or wall-clock events.
 */
export type HistoryStatus = { cursor: number, redo_count: number, generation: number, 
/**
 * The viewer may take back its last decision (`ClientMessage::Undo`): it is its own, and
 * since it nothing hidden was shown, nothing was rolled and no other seat decided. It is
 * not the host's history: `cursor` and `redo_count` say nothing of it.
 */
can_undo?: boolean, };
