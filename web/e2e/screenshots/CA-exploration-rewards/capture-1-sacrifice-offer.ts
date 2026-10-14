import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";
import { shot } from "../_shared/shot";

/** `before` is taken on the commit without the change (the old engine sent no offer details). */
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";
const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;

const CARDS = [
  { card: "cm1", reward: "gain 1 trade good", name: "Core Mine", text: "If you have at least 1 mech on this planet, or if you remove 1 infantry from this planet, gain 1 trade good." },
  { card: "vfs1", reward: "gain 1 command token", name: "Volatile Fuel Source", text: "If you have at least 1 mech on this planet, or if you remove 1 infantry from this planet, gain 1 command token." },
  { card: "exp1", reward: "ready this planet", name: "Expedition", text: "If you have at least 1 mech on this planet, or if you remove 1 infantry from this planet, ready this planet." },
];

for (const [size, viewport] of Object.entries(SIZES)) {
  for (const c of CARDS) {
    test(`sacrifice offer ${c.card} ${size}`, async ({ page }, testInfo) => {
      await page.setViewportSize(viewport);
      const details =
        prefix === "after"
          ? {
              kind: "offer",
              card: { title: c.name, tag: "hazardous exploration card", window: "on exploring this planet", text: c.text },
              facts: [
                { label: "Planet", planet: "mehar_xull", system: "26" },
                { label: "You lose", value: "1 infantry on this planet" },
                { label: "You gain", value: c.reward },
              ],
              captions: {
                remove_infantry: { label: `Remove 1 infantry and ${c.reward}`, hint: "the infantry is returned to your reinforcements" },
                decline: { label: "Keep the infantry", hint: "the card has no effect" },
              },
            }
          : undefined;
      await openMockedGame(page, {
        board: galleryBoard,
        players: [playerWithHand({}), opponent],
        choice: {
          prompt: "remove 1 infantry from this planet to resolve the exploration card",
          context: { subtype: `${c.card}_choose_reward`, source: { Content: c.card } },
          options: [
            { id: "remove_infantry", label: "remove 1 infantry and gain the reward", kind: "explore" },
            { id: "decline", label: "do not remove an infantry", kind: "explore" },
          ],
          ...(details ? { details } : {}),
        },
      });
      await page.getByTestId("choice-option").first().waitFor().catch(() => undefined);
      await page.waitForTimeout(700);
      if (prefix === "after") {
        const body = page.locator("body");
        await expect(body).toContainText(c.name);
        await expect(body).toContainText(c.reward);
        await expect(body).toContainText("1 infantry");
      }
      await shot(page, testInfo, `${prefix}-${c.card}-${size}`);
    });
  }
}
