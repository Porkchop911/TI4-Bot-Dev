// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlanetId } from "./PlanetId.ts";
import type { StructureWithheld } from "./StructureWithheld.ts";
import type { SystemId } from "./SystemId.ts";

/**
 * A structure of [`StructurePlacement::withheld`]: the kind, where, and why.
 */
export type WithheldStructure = { 
/**
 * `pds` or `spacedock`.
 */
unit: string, planet: PlanetId, system: SystemId, reason: StructureWithheld, };
