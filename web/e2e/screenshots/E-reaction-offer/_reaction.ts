import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";

export const other = opponent.id;

/** The trigger exactly as the engine's DecisionTrigger serializes (empty fields left out). */
export const trigger = (kind: string, event_type: string, relation: "when" | "after", rest: Record<string, unknown> = {}) => ({
  kind,
  event_type,
  event_id: 21,
  relation,
  ...rest,
});

/** The outer window offer for one card, shaped like Resolver::pick_with_context. */
export const outerOffer = (card: string, name: string, event: string, relation: "when" | "after") => ({
  id: `reaction:hacan:${event}:${relation}`,
  label: `Play ${name}`,
  kind: "ability",
  payload: { card, card_name: name, event },
});

export const pass = { id: "decline", label: "Pass", kind: "decline" };

export async function openReaction(
  page: Page,
  subtype: string,
  options: unknown[],
  triggerDto: Record<string, unknown> | null,
  source: Record<string, unknown>,
  prompt = "reaction",
) {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: {
      prompt,
      context: { subtype, optional: true, source, ...(triggerDto ? { trigger: triggerDto } : {}) },
      options: options as never,
    },
  });
  await page.getByTestId("reaction-status-bar").waitFor().catch(() => page.waitForTimeout(800));
  await page.waitForTimeout(500);
}
