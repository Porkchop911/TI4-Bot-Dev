// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { GameRequest } from "./GameRequest.ts";
import type { Phase } from "./Phase.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { SeatStaged } from "./SeatStaged.ts";
import type { SeatSwitch } from "./SeatSwitch.ts";
import type { TableSetup } from "./TableSetup.ts";

/**
 * The save of a game ([`HostSession::save_state`]), format [`crate::host_setup::SAVE_FORMAT`]:
 * the game, and every decision of every seat in order, so that a resume gives each of them again
 * and no seat decides again. An undo cuts `decisions` at the answer that it takes back.
 */
export type HostSave = { 
/**
 * [`crate::host_setup::SAVE_FORMAT`]: a save of another format is not read.
 */
format: number, seed: number, players: number, 
/**
 * The game as the host was given it, with the choices that were left open filled in (card
 * set, map, factions...): a save replays with it.
 */
game: GameRequest, 
/**
 * The table of `ti4_host::setup` the game was made from; none for a game of a fixed setup.
 */
table?: TableSetup, 
/**
 * The scenario the game starts from.
 */
scenario?: string, 
/**
 * The rule bot version the game was played with (left out: version 1).
 */
bot_version?: number, 
/**
 * The rules version the game was played with (`GameState::rules_version`; left out:
 * version 0, the rules of a save made before the field existed).
 */
rules_version?: number, 
/**
 * The seats that the page plays at the start (bit 0 is seat `a`).
 */
humans: number, 
/**
 * The option id of every decision of every seat, in order.
 */
decisions: Array<string>, 
/**
 * The page's answers among them (ids), in order.
 */
answers: Array<string>, 
/**
 * The changes of the seats that the page plays, in order.
 */
switches: Array<SeatSwitch>, 
/**
 * The answers that a redo can bring back, next first.
 */
redo: Array<string>, 
/**
 * What each seat has staged and not sent (`ti4_host::staged`).
 */
staged: { [key in PlayerId]: SeatStaged }, 
/**
 * The cards that are never offered, by printed name; an own-turn card is not kept.
 */
never: Array<string>, 
/**
 * The time of each event of the game, in order.
 */
event_times: Array<string>, 
/**
 * The game version of each event, in order.
 */
event_versions: Array<number | null>, version: number, 
/**
 * The history generation of the run.
 */
generation: number, 
/**
 * [`decisions_hash`] of the decisions: a replay tells that the engine still decides the same.
 */
decisions_hash: string, finished: boolean, round: number, phase: Phase, 
/**
 * The engine that saved: its build and the digest of its content.
 */
engine: { build: string, content_digest: string }, };
