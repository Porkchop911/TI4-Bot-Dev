// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * The numbers that fleet supply and capacity are read from, for one seat in one system.
 *
 * [`FleetCounts::standing`] is the one arithmetic of this module: every question about the two
 * limits is answered by building these counts and reading them there. A client that stages units
 * before it sends them (a production) adds what it stages to the counts of the engine and asks
 * the engine for the standing, so it never repeats the rule.
 */
export type FleetCounts = { 
/**
 * Non-fighter ships this seat may keep here (37.1).
 */
fleet_limit: number, 
/**
 * Ships that count against that limit (37.1, 37.1a).
 */
ships: number, 
/**
 * Capacity the ships in the space area provide (16.1).
 */
transport: number, 
/**
 * Capacity that ground forces in the space area use.
 */
carried: number, 
/**
 * Capacity that fighters in the space area use.
 */
fighters: number, 
/**
 * Fighters that the seat's structures in the system hold without capacity (Space Dock).
 */
fighter_support: number, 
/**
 * Fighters that the capacity cannot hold count against the fleet pool (Fighter II).
 */
fighters_to_fleet_pool: boolean, 
/**
 * Such a fighter counts as half a ship (Naalu Hybrid Crystal Fighter II).
 */
half_fighters: boolean, };
