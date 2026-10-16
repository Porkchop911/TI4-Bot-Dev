// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";

/**
 * A die roll result during combat.
 */
export type CombatDieRoll = { player?: PlayerId, unit: string, roll: number, target: number, hit: boolean, };
