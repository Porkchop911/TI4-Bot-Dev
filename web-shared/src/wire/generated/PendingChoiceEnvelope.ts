// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { Choice } from "./Choice.ts";

/**
 * Submission metadata around the engine's wire-serialized choice.
 */
export type PendingChoiceEnvelope = { nonce: string, choice: Choice, };
