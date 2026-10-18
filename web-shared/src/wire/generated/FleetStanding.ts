// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * Fleet supply and transport for one seat in one system, answered together.
 *
 * Together because the two limits are not independent: Fighter II moves fighters the capacity
 * cannot hold onto the fleet pool, so a ground force placed in a space area can push fighters out
 * of capacity and consume fleet supply without a single ship being produced. Answering either
 * question on its own gets that case wrong.
 *
 * On the wire it is the answer of the `fleet_standing` query of the engine of the page, named
 * `FleetStanding` there.
 */
export type FleetStanding = { 
/**
 * Non-fighter ships this seat may keep here (37.1).
 */
fleet_limit: number, 
/**
 * Ships charged against that limit, including the fighters the pool carries.
 */
fleet_charged: number, 
/**
 * Capacity the ships present provide (16.1).
 */
transport: number, 
/**
 * Capacity that fighters and ground forces in the space area consume, after a structure's
 * fighter-only support has excused what it can.
 */
consumed: number, 
/**
 * Fighters the fleet pool carries instead of capacity (Fighter II).
 */
fighters_charged: number, 
/**
 * Capacity-consuming units that cannot legally remain here (16.3).
 */
capacity_excess: number, };
