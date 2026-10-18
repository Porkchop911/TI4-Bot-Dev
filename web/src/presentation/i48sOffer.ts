import type { PendingChoiceDto } from "../protocol/types.ts";

/** The decision subtype of the L1Z1X agent I48S offered after a system activation. */
export const I48S_SUBTYPE = "l1z1x_agent_swap";

export function isI48sOffer(choice: PendingChoiceDto | null | undefined): boolean {
  return choice?.context?.subtype === I48S_SUBTYPE;
}

/**
 * The option that declines I48S when the client may answer it for `viewerSeat`: the offer is the
 * viewer's own (the agent's owner is the actor) and carries a decline answer. Otherwise null.
 */
export function i48sDeclineOption(
  choice: PendingChoiceDto | null | undefined,
  viewerSeat: string | null | undefined,
): string | null {
  if (!choice || !viewerSeat || choice.actor !== viewerSeat || !isI48sOffer(choice)) return null;
  return choice.options.find((o) => o.kind === "decline")?.id ?? null;
}
