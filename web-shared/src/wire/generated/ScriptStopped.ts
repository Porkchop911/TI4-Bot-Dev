// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { DraftStep } from "./DraftStep.ts";

/**
 * Where a script that was sent to the live game stopped before its end.
 */
export type ScriptStopped = { 
/**
 * How many entries of the script are in the game. 0 when the script was refused: the game
 * is as before.
 */
applied: number, 
/**
 * The entries that are not in the game.
 */
remaining: Array<DraftStep>, 
/**
 * True when the game asked something else first, or went on without the rest; false when
 * it refused an entry and nothing was sent.
 */
interrupted: boolean, reason: string, };
