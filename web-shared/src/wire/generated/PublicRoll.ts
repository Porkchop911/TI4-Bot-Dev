// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";

/**
 * One public die roll: who rolled, what for, what showed, and what it did.
 *
 * Built from the engine's roll history after the fact, so publishing it cannot change any
 * decision or draw. Rolls that decide hidden information (which card Spy takes) are not made
 * public; see [`public_roll`].
 */
export type PublicRoll = { 
/**
 * Position in the game's roll history. A restart replays the same rolls; this is how the
 * replay is told apart from a new roll.
 */
index: number, 
/**
 * Whose dice these are, when the engine said.
 */
roller?: PlayerId, 
/**
 * The engine's tag for the roll ("gravity rift", "space combat").
 */
reason: string, 
/**
 * The tag in words.
 */
label: string, faces: Array<number>, 
/**
 * The face a die needs to hit, for hit rolls.
 */
hits_on?: number, hits?: number, 
/**
 * Positions of the dice that are rerolls.
 */
rerolled?: Array<number>, 
/**
 * The system it happened in: a rift exited on the way, else the active system.
 */
system?: string, 
/**
 * What it did ("ship destroyed", "2 hits").
 */
outcome?: string, 
/**
 * The unit that rolled, by type id, when the engine said: the ship of a gravity rift roll,
 * the gun of a space cannon roll. Its owner is `roller`.
 */
unit?: string, 
/**
 * The unit had sustained damage.
 */
damaged?: boolean, 
/**
 * The system the ship of a gravity rift roll started from.
 */
origin?: string, 
/**
 * What the ship of a gravity rift roll carried, by type id: it is lost with the ship.
 */
cargo?: Array<string>, 
/**
 * The seat whose ships a space cannon roll fires at.
 */
target?: PlayerId, };
