// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";

/**
 * The unit behind a roll, for the public roll feed: the ship of a gravity rift roll, the gun of a
 * space cannon roll. Its owner is the roller. Presentation only.
 */
export type RollSubject = { 
/**
 * The content id of the unit's type (`carrier`, `pds2`). `None` for dice that are no unit's
 * own (the SPACE CANNON of an attachment).
 */
unit?: string, 
/**
 * The unit had sustained damage.
 */
damaged?: boolean, 
/**
 * The system the unit started from, when it moved (a ship that left a gravity rift).
 */
origin?: string, 
/**
 * The units the ship carried, by type id: they share its fate.
 */
cargo?: Array<string>, 
/**
 * The seat whose ships the dice fire at.
 */
target?: PlayerId, };
