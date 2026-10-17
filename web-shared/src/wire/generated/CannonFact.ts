// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One gun of the space cannon offense of the active system: a unit in the system or one whose
 * SPACE CANNON reaches it, or the SPACE CANNON of an attachment.
 */
export type CannonFact = { seat: string, 
/**
 * The unit, by type id; `None` for an attachment, which names its `planet`.
 */
unit?: string, planet?: string, dice: number, hits_on: number, 
/**
 * The seat whose ships the gun fires at: the seat that moves, or for its own guns the seat
 * with ships in the system.
 */
target?: string, };
