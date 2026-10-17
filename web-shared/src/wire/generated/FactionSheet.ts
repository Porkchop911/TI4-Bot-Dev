// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { HomePlanet } from "./HomePlanet.ts";
import type { StartingUnits } from "./StartingUnits.ts";
import type { TechnologyChoice } from "./TechnologyChoice.ts";

/**
 * What a table says of a faction wherever it names a seat.
 */
export type FactionSheet = { 
/**
 * The name that a table calls the faction by: "Sol".
 */
name: string, 
/**
 * The name as printed: "The Federation of Sol".
 */
title: string, 
/**
 * The commodity value of the faction sheet.
 */
commodities: number, 
/**
 * The agent, the commander and the hero, by id ([`ComponentCatalog::leaders`]).
 */
leaders: Array<string>, 
/**
 * The technologies that the faction begins with, by id ([`ComponentCatalog::technologies`]).
 */
starting_technologies: Array<string>, 
/**
 * A faction that chooses its starting technologies: how many, and of which.
 */
starting_technology_choice?: TechnologyChoice, 
/**
 * The faction technologies, by id.
 */
faction_technologies: Array<string>, 
/**
 * The units that the faction begins with, in the order of the sheet.
 */
starting_fleet: Array<StartingUnits>, 
/**
 * The planets of the home system.
 */
home_planets: Array<HomePlanet>, };
