// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlanetId } from "./PlanetId.ts";
import type { StructurePlacement } from "./StructurePlacement.ts";
import type { StructureSupply } from "./StructureSupply.ts";

/**
 * The look-ahead of a placement of structures (`details.structures`): every placement of the
 * card from this question on as the game will offer it, so that a client can stage them all at
 * once (display only).
 */
export type StructureHints = { placements: Array<StructurePlacement>, 
/**
 * The player's structures on each planet of an option or of a withheld structure, by kind
 * (`pds`, `spacedock`).
 */
planets: { [key in PlanetId]: { [key in string]: number } }, 
/**
 * The PDS and the space docks on the board, against the box, by kind.
 */
supply: { [key in string]: StructureSupply }, };
