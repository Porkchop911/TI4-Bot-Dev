// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { StagedSlot } from "./StagedSlot.ts";
import type { StagedValue } from "./StagedValue.ts";

/**
 * What the seat of a connection has staged. Sent only to the connections of that seat that
 * asked for it (`ClientMessage::Subscribe::staged`): after the snapshot, after every write of
 * any of them, and to a connection whose write was stale. Carries no game version on purpose: a
 * stale-version filter must never drop it; the newest message is the one with the highest
 * `revision`.
 */
export type SeatStagedMsg = { protocol_version: number, game_id: string, 
/**
 * The revision of the newest write of any slot (0: never written).
 */
revision: number, 
/**
 * The slots that were ever written; a cleared slot has no value.
 */
slots: { [key in StagedSlot]?: StagedValue }, 
/**
 * The connection whose write this is, so that it can skip its own echo. Absent in the
 * message after the snapshot and in the answer to a stale write.
 */
writer?: number, 
/**
 * The id of the connection that is sent this message, in the message after its snapshot
 * only: the `writer` of its own writes.
 */
connection?: number, };
