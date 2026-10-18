// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { Phrase } from "./Phrase.ts";

/**
 * `details.phrases` of a question: the prompt and the options (by id) whose text names
 * something by its id. Options that name nothing are absent.
 */
export type PhraseHints = { prompt?: Phrase, options: { [key in string]: Phrase }, };
