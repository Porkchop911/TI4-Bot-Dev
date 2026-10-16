// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { DraftStep } from "./DraftStep.ts";
import type { RefusalCode } from "./RefusalCode.ts";
import type { StopReason } from "./StopReason.ts";

/**
 * Why a run ended.
 */
export type DraftStop = { "kind": "open" } | { "kind": "complete" } | { "kind": "refused", step: number, reason: string, code: RefusalCode, entry?: DraftStep, } | { "kind": "stopped", reason: StopReason, } | { "kind": "failed", reason: string, };
