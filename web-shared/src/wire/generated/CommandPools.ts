// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * The three pools of a command sheet.
 */
export type CommandPools = { tactic: number, fleet: number, strategic: number, 
/**
 * The non-fighter ships per system that the seat may keep beyond its fleet pool
 * ([`crate::faction_abilities::armada_bonus`]: Letnev's Armada, 2). Left out when 0.
 */
fleet_supply_bonus?: number, };
