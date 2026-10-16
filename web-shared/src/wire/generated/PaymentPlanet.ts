// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlanetId } from "./PlanetId.ts";

/**
 * A planet that pays a bill, as the look-ahead of a payment lists it.
 */
export type PaymentPlanet = { id: PlanetId, 
/**
 * What the planet pays with its best face.
 */
worth: number, 
/**
 * The kind of value that the best face spends, when it is not the kind of the bill:
 * "influence" for resources (Archon's Gift). The payment option of the planet is then
 * `exhaust|<planet>|<face>`.
 */
face?: string, };
