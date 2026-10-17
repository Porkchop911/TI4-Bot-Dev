// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * Outcome statistics from [`arena`] rollouts. The four rates are fractions of the rollouts,
 * exclusive, and sum to 1: the attacker destroys every ship of the defender, the defender
 * destroys every ship of the attacker, both fleets are destroyed, or the rollout did not end
 * (both fleets still had ships at [`arena::ROUND_CAP`]). A rollout that did not end is only in
 * `unresolved_rate`; what it means is the client's. The survivors and the fielded units are by
 * unit id.
 */
export type BattleResponse = { simulations: number, attacker_win_rate: number, defender_win_rate: number, 
/**
 * Both fleets are destroyed. Leaves out the rollouts that did not end.
 */
mutual_destruction_rate: number, 
/**
 * The rollouts that did not end: both fleets still had ships at the round cap.
 */
unresolved_rate: number, average_rounds: number, attacker_expected_survivors: { [key in string]: number }, defender_expected_survivors: { [key in string]: number }, attacker_fielded: { [key in string]: number }, defender_fielded: { [key in string]: number }, };
