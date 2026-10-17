// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { EventVisibility } from "./EventVisibility.ts";
import type { GameEventKind } from "./GameEventKind.ts";
import type { MovementFact } from "./MovementFact.ts";
import type { Phase } from "./Phase.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { PublicRoll } from "./PublicRoll.ts";
import type { SeatDecisionDetail } from "./SeatDecisionDetail.ts";

/**
 * Authoritative, auditable event log entry recorded during game execution.
 */
export type GameEvent = { id: string, timestamp: string, version?: number, visibility: EventVisibility, event: GameEventKind, decision_count?: number, batch_id?: string, batch_start_cursor?: number, batch_end_cursor?: number, action_id?: string, action_start_cursor?: number, actor?: PlayerId, round?: number, phase?: Phase, action_type?: string, action_actor?: PlayerId, stage?: string, detail?: string, movement?: MovementFact, 
/**
 * Stored only in authoritative history; stripped from every projected event.
 */
seat_detail?: SeatDecisionDetail, 
/**
 * Only populated on the acting seat's projected copy.
 */
private_detail?: string, 
/**
 * The roll behind a [`GameEventKind::DiceRolled`] event. Public: dice are rolled in the open.
 */
roll?: PublicRoll, };
