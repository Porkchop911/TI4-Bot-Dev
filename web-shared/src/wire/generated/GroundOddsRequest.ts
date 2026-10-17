// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { GroundOddsSide } from "./GroundOddsSide.ts";

/**
 * The two sides of a ground combat, and the ships of the attacker that bombard.
 */
export type GroundOddsRequest = { attacker: GroundOddsSide, defender: GroundOddsSide, 
/**
 * Legal bombarding ships that fire Harrow after rounds (never before landing).
 */
harrow?: { [key in string]: number }, 
/**
 * Number of rollouts (defaults to 2000, at most 50000).
 */
simulations?: number, };
