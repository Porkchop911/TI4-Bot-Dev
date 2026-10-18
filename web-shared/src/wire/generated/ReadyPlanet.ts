// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlanetId } from "./PlanetId.ts";

/**
 * An exhausted planet that can be readied, with its values. The values are null when the
 * content has no record of the planet.
 */
export type ReadyPlanet = { id: PlanetId, name: string, resources: number | null, influence: number | null, };
