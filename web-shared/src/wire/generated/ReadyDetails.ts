// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ReadyPlanet } from "./ReadyPlanet.ts";
import type { ReadySpender } from "./ReadySpender.ts";

/**
 * What a seat weighing "ready planets" needs in front of it (`details.ready_hints`).
 */
export type ReadyDetails = { 
/**
 * The exhausted planets that the seat controls, which it could ready.
 */
planets: Array<ReadyPlanet>, 
/**
 * The other strategy cards that spend resources or influence (Leadership, Technology,
 * Warfare), in that order.
 */
spenders: Array<ReadySpender>, };
