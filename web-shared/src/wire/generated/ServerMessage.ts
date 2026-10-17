// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ActionAcceptedMsg } from "./ActionAcceptedMsg.ts";
import type { ActionRejectedMsg } from "./ActionRejectedMsg.ts";
import type { DraftAppliedMsg } from "./DraftAppliedMsg.ts";
import type { DraftOutcomeMsg } from "./DraftOutcomeMsg.ts";
import type { GameEventMsg } from "./GameEventMsg.ts";
import type { GameOverMsg } from "./GameOverMsg.ts";
import type { InitialSnapshotMsg } from "./InitialSnapshotMsg.ts";
import type { PendingChoiceMsg } from "./PendingChoiceMsg.ts";
import type { PongMsg } from "./PongMsg.ts";
import type { ProtocolErrorMsg } from "./ProtocolErrorMsg.ts";
import type { ReachIntoMsg } from "./ReachIntoMsg.ts";
import type { ReactionIntentStateMsg } from "./ReactionIntentStateMsg.ts";
import type { SeatStagedMsg } from "./SeatStagedMsg.ts";
import type { SecondaryPreviewMsg } from "./SecondaryPreviewMsg.ts";
import type { StateUpdateMsg } from "./StateUpdateMsg.ts";
import type { TurnStatusMsg } from "./TurnStatusMsg.ts";

/**
 * Messages emitted from server to client.
 */
export type ServerMessage = { "type": "initial_snapshot" } & InitialSnapshotMsg | { "type": "state_update" } & StateUpdateMsg | { "type": "pending_choice" } & PendingChoiceMsg | { "type": "turn_status" } & TurnStatusMsg | { "type": "action_accepted" } & ActionAcceptedMsg | { "type": "action_rejected" } & ActionRejectedMsg | { "type": "error" } & ProtocolErrorMsg | { "type": "game_over" } & GameOverMsg | { "type": "pong" } & PongMsg | { "type": "event" } & GameEventMsg | { "type": "draft_outcome" } & DraftOutcomeMsg | { "type": "draft_applied" } & DraftAppliedMsg | { "type": "reaction_intent_state" } & ReactionIntentStateMsg | { "type": "secondary_preview" } & SecondaryPreviewMsg | { "type": "reach_into" } & ReachIntoMsg | { "type": "seat_staged" } & SeatStagedMsg;
