import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";

export const other = opponent.id;

/** One faction reaction as the engine offers it: the label is the raw ability id. */
export const abilityOffer = (id: string) => ({ id, label: `Play ${id}`, kind: "ability" });
export const pass = { id: "decline", label: "Pass", kind: "decline" };

export async function openAbility(page: Page, event: string, relation: "when" | "after", id: string) {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: {
      prompt: `${relation} ${event}`,
      context: {
        subtype: `reaction_${relation}_${event}`,
        optional: true,
        source: { Reaction: event },
        trigger: { kind: "generic", event_type: event, event_id: 21, relation, actor: other },
      },
      options: [abilityOffer(id), pass] as never,
    },
  });
  await page.getByTestId("reaction-status-bar").waitFor().catch(() => page.waitForTimeout(800));
  await page.waitForTimeout(500);
}
