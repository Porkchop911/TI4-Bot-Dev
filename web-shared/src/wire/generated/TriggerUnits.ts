// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";

/**
 * A public unit count named by a trigger: owner, unit type and how many.
 */
export type TriggerUnits = { owner: PlayerId, unit_type: string, count: number, };
