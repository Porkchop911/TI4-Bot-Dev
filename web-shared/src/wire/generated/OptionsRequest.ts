// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ModelEntry } from "./ModelEntry.ts";

/**
 * The request of [`ti4_setup_options`].
 */
export type OptionsRequest = { 
/**
 * The number of seats whose maps are listed; 0 (or left out): every map.
 */
players: number, 
/**
 * The models that the page can load (from the server): each is the kind `mlp:<name>`.
 */
models: Array<ModelEntry>, };
