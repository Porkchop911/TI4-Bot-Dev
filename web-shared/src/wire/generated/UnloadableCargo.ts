// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { Unloadable } from "./Unloadable.ts";

/**
 * Units of one kind in one place that use capacity, in a system that a ship of the movement
 * starts in or passes, that no ship can load: they stay where they are.
 */
export type UnloadableCargo = { system: string, 
/**
 * The planet, or `None` for the space area.
 */
source: string | null, unit: string, damaged: boolean, count: number, reason: Unloadable, };
