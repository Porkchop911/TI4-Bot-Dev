// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ScenarioMap } from "./ScenarioMap.ts";

/**
 * A prepared game that a host can start.
 */
export type Scenario = { id: string, title: string, description: string, 
/**
 * What the scenario is about, for a list of the scenarios ("Combat", "Strategy", ...).
 */
category: string, 
/**
 * The game that the scenario is made for: its board is placed on the map of this game.
 */
seed: number, players: number, 
/**
 * The places of the seats that people play; the rule bot plays the others.
 */
humans: Array<number>, 
/**
 * The scenario finds its systems by the places of the seats, on any board: a table plays
 * it on its own seed. `seed` is then the seed of a table that names none.
 */
any_seed: boolean, map: ScenarioMap, };
