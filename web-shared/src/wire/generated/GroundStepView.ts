// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { InvasionStepView } from "./InvasionStepView.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { SystemId } from "./SystemId.ts";

/**
 * A step of an invasion of the turn as it resolved ([`ti4_model::state::GroundStepRecord`]).
 */
export type GroundStepView = { system_id: SystemId, invasion_seq: number, invader: PlayerId, step: InvasionStepView, };
