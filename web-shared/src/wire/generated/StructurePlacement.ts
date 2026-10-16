// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { StructureSpot } from "./StructureSpot.ts";
import type { WithheldStructure } from "./WithheldStructure.ts";

/**
 * One placement of [`StructureHints`], as its question will offer it.
 */
export type StructurePlacement = { 
/**
 * Which placement of the card's sequence it is (`step` of `of`); none for a lone one.
 */
step: number | null, of: number | null, only_pds: boolean, options: Array<StructureSpot>, 
/**
 * Every structure of every planet that the player controls that this placement does not
 * offer now, with why. A kind on a planet is in `options` or here, never both.
 */
withheld: Array<WithheldStructure>, 
/**
 * For each option of the placement before, the options of this one that choosing it takes
 * away (a second PDS where the planet then has two, the last one of the box). An option
 * that takes nothing away is not listed.
 */
without: { [key in string]: Array<string> }, };
