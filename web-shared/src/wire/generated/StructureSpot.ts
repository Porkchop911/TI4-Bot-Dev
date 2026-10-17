// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlanetId } from "./PlanetId.ts";
import type { SystemId } from "./SystemId.ts";

/**
 * An option of a [`StructurePlacement`]: its id, the kind it places, and where.
 */
export type StructureSpot = { id: string, 
/**
 * `pds` or `spacedock`.
 */
unit: string, planet: PlanetId, system: SystemId, };
