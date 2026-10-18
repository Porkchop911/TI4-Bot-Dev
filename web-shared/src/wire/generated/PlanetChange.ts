// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlanetId } from "./PlanetId.ts";
import type { PlanetView } from "./PlanetView.ts";
import type { SystemId } from "./SystemId.ts";

/**
 * A planet, before and after (`None`: not in the view of its system).
 */
export type PlanetChange = { system: SystemId, planet: PlanetId, before: PlanetView | null, after: PlanetView | null, };
