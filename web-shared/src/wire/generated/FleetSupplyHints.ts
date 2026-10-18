// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * The fleet supply of the seat in the system of a movement or production choice (display only,
 * `details.fleet_supply`): the ships charged against it and its limit (37), so a client can
 * show the room left.
 */
export type FleetSupplyHints = { 
/**
 * Ships charged against the limit ([`Standing::fleet_charged`]).
 */
used: number, 
/**
 * The enforced limit ([`Standing::fleet_limit`]), the bonus included.
 */
limit: number, 
/**
 * The limit does not apply (Darktalon Treilla, [`is_unlimited`]). Left out when false.
 */
unlimited?: boolean, 
/**
 * The part of `limit` beyond the fleet pool
 * ([`crate::faction_abilities::armada_bonus`]: Letnev's Armada, 2). Left out when 0.
 */
fleet_supply_bonus?: number, };
