// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";

/**
 * One public objective as printed, with the seats that scored it.
 */
export type PublicObjective = { id: string, name: string, 
/**
 * What the objective is worth: 1 for stage I, 2 for stage II.
 */
points: number, text: string, 
/**
 * The seats that scored it, sorted by id.
 */
scored: Array<PlayerId>, };
