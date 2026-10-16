// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ResearchSpecialty } from "./ResearchSpecialty.ts";
import type { ResearchTechnology } from "./ResearchTechnology.ts";

/**
 * The technology tree of the seat that is asked to research.
 */
export type ResearchFacts = { 
/**
 * Every technology of the game that the seat could own: the generic ones that its faction
 * has not replaced, its faction technologies, and what it owns besides.
 */
technologies: Array<ResearchTechnology>, specialties: Array<ResearchSpecialty>, };
