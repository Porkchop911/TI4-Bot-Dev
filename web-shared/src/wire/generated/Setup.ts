// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { Bots } from "./Bots.ts";
import type { SpeakerOrder } from "./SpeakerOrder.ts";

/**
 * The table of a game. Everything the game's opening state is made from; a save carries it.
 */
export type Setup = { game_id: string, seed: number, players: number, 
/**
 * 0-based place of the person. Left out: drawn from the seed.
 */
human_seat?: number, 
/**
 * A faction id of `seating::LOBBY_FACTIONS`. Left out: drawn.
 */
human_faction?: string, 
/**
 * One of `card_set::OFFERED`. Left out: `card_set::DEFAULT`.
 */
card_set?: string, 
/**
 * A template alias, `"random"` for the seeded random board. Left out: the template that is
 * recommended for the number of players.
 */
map?: string, 
/**
 * A start preset of `ti4_host::preset` (a test table). Not offered by the form.
 */
preset?: string, 
/**
 * `"seat"` (the first seat opens) or `"random"`. Left out: random.
 */
speaker?: SpeakerOrder, 
/**
 * A prepared game of `ti4_host::view::scenario` (its players win, and its seed unless it
 * plays on any seed).
 */
scenario?: string, 
/**
 * Who plays the seats of the others. Left out: the rule bot.
 */
bots?: Bots, };
