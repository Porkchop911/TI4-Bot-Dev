/**
 * Client-side memory of one negotiation, so the desk can chain what the engine asks in two steps.
 *
 * Choosing "Counter-offer" is one submission; the engine then asks the same seat to propose from
 * the other chair. The mirrored offer is carried across that gap here and pre-fills the staging
 * desk, which then validates it against the freshly listed deals. Presentation state only: nothing
 * here is sent anywhere and it is dropped when a negotiation ends or the page reloads.
 */
import type { Staged } from "./tradeStaging.ts";

interface Memory {
  /** A mirrored offer waiting for the counter-proposal desk (consumed once). */
  prefill: { partnerSeat: string; staged: Staged } | null;
  /** The seat this viewer last proposed to; a later answer from it is already a counter. */
  proposedTo: string | null;
}

let memory: Memory = { prefill: null, proposedTo: null };

export const rememberCounter = (partnerSeat: string, staged: Staged): void => {
  memory = { ...memory, prefill: { partnerSeat, staged } };
};

/** Takes the pending prefill for this partner (null when none, or meant for someone else). */
export function takeCounterPrefill(partnerSeat: string | null): Staged | null {
  const p = memory.prefill;
  if (!p || p.partnerSeat !== partnerSeat) return null;
  memory = { ...memory, prefill: null };
  return p.staged;
}

export const rememberProposal = (partnerSeat: string | null): void => {
  memory = { ...memory, proposedTo: partnerSeat };
};

/** True when this viewer proposed to `seat` earlier, so an answer from it is the one counter. */
export const answeringACounter = (seat: string | null): boolean =>
  !!seat && memory.proposedTo === seat;

export const forgetNegotiation = (): void => {
  memory = { prefill: null, proposedTo: null };
};
