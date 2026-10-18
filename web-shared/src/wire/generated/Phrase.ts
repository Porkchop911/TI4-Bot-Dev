// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { Subject } from "./Subject.ts";

/**
 * A text of the engine as a template: `text` is the label or the prompt with `{key}` where it
 * names `refs[key]`.
 */
export type Phrase = { text: string, refs: { [key in string]: Subject }, };
