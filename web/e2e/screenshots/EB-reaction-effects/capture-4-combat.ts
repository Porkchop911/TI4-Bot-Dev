import { expect, test } from "@playwright/test";
import { completedCombatBoard } from "../_shared/fixtures";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";
import { shot } from "../_shared/shot";
import { SCENES } from "./cards";
import { SIZES, offerId, pass, prefix } from "./scene";

// The same two combat cards inside the space combat overlay, which has its own offer rows: during a
// fight the reaction window is shown there, not in the reaction bar.
const COMBAT_KEYS = ["direct-hit", "shields-holding"] as const;

for (const key of COMBAT_KEYS) {
  const scene = SCENES.find((candidate) => candidate.key === key)!;
  for (const size of SIZES) {
    test(`${prefix}-combat-${key}-${size.label}`, async ({ page }, testInfo) => {
      await page.setViewportSize({ width: size.width, height: size.height });
      const id = offerId(scene);
      await openMockedGame(page, {
        players: [playerWithHand({ held_action_cards: [scene.alias], action_cards_count: 1 }), opponent],
        board: completedCombatBoard({ phase: "assign_hits" }),
        choice: {
          prompt: `${scene.relation} ${scene.event}`,
          context: {
            subtype: `reaction_${scene.relation}_${scene.event}`,
            optional: true,
            space_battle: true,
            source: { Reaction: scene.event },
            trigger: scene.trigger,
            previews: [{ option: id, card: scene.alias, ...scene.preview }],
          },
          options: [
            { id, label: `Play ${scene.name}`, kind: "ability", payload: { card: scene.alias, card_name: scene.name, event: scene.event } },
            pass,
          ],
        },
      });
      await page.waitForTimeout(800);
      if (prefix === "after") {
        const effect = page.getByTestId("combat-reaction-effect");
        await expect(effect).toBeVisible();
        expect(await effect.textContent()).toMatch(/Cancels|Destroys/);
      }
      await shot(page, testInfo, `${prefix}-combat-${key}-${size.label}`);
    });
  }
}
