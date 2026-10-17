// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";
import type { SystemId } from "./SystemId.ts";

/**
 * The command tokens in a system, before and after.
 */
export type TokensChange = { system: SystemId, before: Array<PlayerId>, after: Array<PlayerId>, };
