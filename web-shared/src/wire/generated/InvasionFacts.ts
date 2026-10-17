// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { LandingPlanet } from "./LandingPlanet.ts";
import type { LandingPool } from "./LandingPool.ts";

export type InvasionFacts = { active: string, forces: Array<LandingPool>, 
/**
 * Every planet of the active system, in the order of the system.
 */
planets: Array<LandingPlanet>, };
