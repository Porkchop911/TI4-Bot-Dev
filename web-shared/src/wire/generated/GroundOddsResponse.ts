// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { GroundRoll } from "./GroundRoll.ts";

/**
 * Outcome of the rollouts of a ground combat. The rates are fractions of the rollouts and sum
 * to 1: the attacker takes the planet, the defender holds it with ground forces left, or both
 * sides are destroyed (the defender keeps the planet, LRR 49.5d). The survivors and the fielded
 * ground forces are by unit id.
 */
export type GroundOddsResponse = { simulations: number, attacker_win_rate: number, 
/**
 * Includes the rollouts that did not end ([`GroundOddsResponse::unresolved_rate`]).
 */
defender_win_rate: number, mutual_destruction_rate: number, 
/**
 * The rollouts that did not end: the invasion fails.
 */
unresolved_rate: number, 
/**
 * Rounds of ground combat, 0 when no round is fought.
 */
average_rounds: number, attacker_expected_survivors: { [key in string]: number }, defender_expected_survivors: { [key in string]: number }, attacker_fielded: { [key in string]: number }, defender_fielded: { [key in string]: number }, 
/**
 * What the fielded ground forces of each side roll in a combat round, by unit id.
 */
attacker_rolls: { [key in string]: GroundRoll }, defender_rolls: { [key in string]: GroundRoll }, };
