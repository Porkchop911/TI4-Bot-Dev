// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One unit card as printed: its numbers and its text. A value that the card does not print is
 * left out.
 */
export type UnitRecord = { id: string, 
/**
 * The unit type: `carrier`, `flagship`, `spacedock`, …
 */
base_type: string, name: string, subtitle?: string, 
/**
 * The faction of a unit of a faction. Empty for a generic unit.
 */
faction?: string, source?: string, 
/**
 * The cost of one unit: 0.5 for a unit that is bought two for one resource.
 */
cost?: number, move_value?: number, capacity_value?: number, combat_hits_on?: number, combat_die_count?: number, sustain_damage?: boolean, afb_hits_on?: number, afb_die_count?: number, bombard_hits_on?: number, bombard_die_count?: number, space_cannon_hits_on?: number, space_cannon_die_count?: number, deep_space_cannon?: boolean, planetary_shield?: boolean, disables_planetary_shield?: boolean, 
/**
 * The PRODUCTION value as printed: a number (`"5"`), or what is added to the resources of
 * the planet (`"+2"`).
 */
production_value?: string, 
/**
 * The technology that makes this the card of the unit.
 */
required_tech_id?: string, upgrades_from_unit_id?: string, upgrades_to_unit_id?: string, 
/**
 * The text of the card, one line for each ability.
 */
ability?: string, };
