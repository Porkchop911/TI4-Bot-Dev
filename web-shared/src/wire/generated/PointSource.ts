// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";
import type { PointSourceKind } from "./PointSourceKind.ts";

/**
 * One source of points that is not a public objective, with what each seat has from it.
 */
export type PointSource = { kind: PointSourceKind, 
/**
 * The printed name of the agenda, relic, card or planet. Empty for the first four kinds:
 * there is one row of each.
 */
name: string, 
/**
 * The points of each seat, gains and losses together. A seat with none is left out.
 */
points: { [key in PlayerId]: number }, 
/**
 * The printed text of the agenda, relic, card or planet. Empty for the first four kinds.
 */
text?: string, 
/**
 * For [`PointSourceKind::Support`]: whose notes the seat has, sorted by id.
 */
from?: { [key in PlayerId]: Array<PlayerId> }, };
