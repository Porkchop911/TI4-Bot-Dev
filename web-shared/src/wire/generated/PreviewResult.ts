// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PreviewRefusal } from "./PreviewRefusal.ts";
import type { SecondaryPreview } from "./SecondaryPreview.ts";

/**
 * What a preview request resolved to.
 */
export type PreviewResult = { "result": "preview", preview: SecondaryPreview, } | { "result": "refused", reason: PreviewRefusal, detail: string, };
