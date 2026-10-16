// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";
import type { StrategyCardId } from "./StrategyCardId.ts";

/**
 * What a disposable attempt previews: the next tactical action of the seat, or its secondary
 * of the strategic action that is resolved (`secondary` of an update). The scope picks the
 * engine preparation and the audited offers; the gate, script and lifecycle are shared.
 */
export type PlanScope = { "kind": "tactical" } | { "kind": "secondary", primary: PlayerId, card: StrategyCardId, };
