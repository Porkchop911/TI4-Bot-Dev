// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ComponentSource } from "./ComponentSource.ts";

/**
 * The action a turn took.
 */
export type TurnAction = { "type": "tactical" } | { "type": "strategic", card?: string, } | { "type": "component", source: ComponentSource, } | { "type": "pass" };
