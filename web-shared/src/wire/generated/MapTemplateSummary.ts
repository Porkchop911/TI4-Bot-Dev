// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * Summary of a map template for listing.
 */
export type MapTemplateSummary = { alias: string, 
/**
 * Display name, when the template has one (community layouts do).
 */
name?: string, author: string, player_count: number, 
/**
 * Whether the template builds under the shipped content (always filled in by the server).
 */
buildable: boolean, 
/**
 * Main-map systems (hyperlane tiles excluded); 0 when the template does not build.
 */
systems: number, hyperlanes: boolean, 
/**
 * The template a lobby of this size starts with when nobody chooses.
 */
recommended: boolean, };
