// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { Choice } from "./Choice.ts";
import type { PaymentPreview } from "./PaymentPreview.ts";
import type { SecondaryBlocker } from "./SecondaryBlocker.ts";
import type { SkippedPrompt } from "./SkippedPrompt.ts";

/**
 * What the follower would be asked, as of the position given.
 */
export type SecondaryPreview = { "status": "question", choice: Choice, step: number, 
/**
 * Technology: how the engine would pay the 4 resources once a technology is chosen.
 */
payment?: PaymentPreview, 
/**
 * Faction prompts the real flow would put first that the preview declined to get here.
 */
skipped?: Array<SkippedPrompt>, } | { "status": "would_not_be_asked", blocker: SecondaryBlocker, } | { "status": "complete", skipped?: Array<SkippedPrompt>, 
/**
 * Scripted answers after the first that the flow never asked for (it ended earlier than
 * the script assumed: a purchase of more tokens than the seat can afford). Payment
 * answers for a payment the engine settled by itself (a lone option) do not count.
 */
unused_answers?: number, } | { "status": "rejected", 
/**
 * Index into the given `answers` of the answer that is not offered.
 */
at: number, answer: string, choice: Choice, } | { "status": "unavailable", detail: string, };
