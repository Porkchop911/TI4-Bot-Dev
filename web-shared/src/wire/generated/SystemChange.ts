// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { SystemId } from "./SystemId.ts";

/**
 * A system that is on the board after the step and was not before, or the other way.
 */
export type SystemChange = { system: SystemId, before: boolean, after: boolean, };
