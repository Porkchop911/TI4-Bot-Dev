// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { DraftStep } from "./DraftStep.ts";
import type { RefusalCode } from "./RefusalCode.ts";

/**
 * Which entry of a script the game refused, and why: what the page marks. The entry names the
 * unit, its system or the planet; the text of the refusal stays for a person.
 */
export type EntryRefusal = { 
/**
 * The place of the entry in the script, in the order the game played it (a payment may
 * have moved to the purchase that it pays).
 */
step: number, code: RefusalCode, 
/**
 * The entry itself. `None` when the script has none at `step`.
 */
entry?: DraftStep, };
