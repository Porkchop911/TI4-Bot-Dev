// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * A game of a fixed setup: a seeded game on the recommended map for its count, or a scenario
 * of `ti4_host::view::scenario` (its seed and its count win). Seat `i` has the id
 * [`SEATS`]`[i]`.
 */
export type Local = { seed: number, players: number, 
/**
 * The seats that the page plays (bit 0 is seat `a`).
 */
humans: number, scenario?: string, 
/**
 * The kind of the bot of each seat where the page does not play it (`smart`, `mlp:<name>`):
 * it decides from the seed, the seat and the number of the decision, as on the server. A
 * seat with none (or `random`, or past the end) decides at random.
 */
bots: ("smart" | "random" | `mlp:${string}` | null)[], };
