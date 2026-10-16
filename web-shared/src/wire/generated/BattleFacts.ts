// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { CombatSide } from "./CombatSide.ts";

/**
 * The space combat in the active system: the seat that defends, and both sides. The attacker
 * is the seat that moves; the ships it moves in are added by the client.
 */
export type BattleFacts = { 
/**
 * The seat that defends.
 */
defender: string, 
/**
 * The seat that moves, with what it has there already.
 */
attacker: CombatSide, defender_side: CombatSide, };
