// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * Why a placement of structures does not offer a kind on a planet that the player controls:
 * the first of its checks that excluded it ([`StructurePlacement::withheld`]).
 */
export type StructureWithheld = "space_station" | "pds_only" | "demilitarized" | "limit" | "banned" | "supply";
