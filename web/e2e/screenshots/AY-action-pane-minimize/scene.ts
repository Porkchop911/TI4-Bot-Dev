import { expect, type Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";

/** Which side of the change a run is: `before` is taken on the commit without the minimize control. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const PHONES = [
  { name: "390x844", width: 390, height: 844 },
  { name: "360x740", width: 360, height: 740 },
];

const tokens = { tactic: 3, fleet: 3, strategy: 2 };

/** The viewer's own turn menu on a phone; the stored preference is absent (fresh context). */
export async function openTurnMenu(page: Page, width: number, height: number) {
  await page.setViewportSize({ width, height });
  const game = await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: {
      prompt: "action phase",
      options: [
        { id: "tactical", label: "take a tactical action", kind: "action" },
        { id: "strategic|pok3politics", label: "take the strategic action of 3. Politics", kind: "action" },
        { id: "pass", label: "pass", kind: "action" },
      ],
      details: {
        kind: "turn_menu",
        closing: false,
        tokens,
        strategy_cards: [{ card: "pok3politics", used: false, option: "strategic|pok3politics" }],
        partners: [],
      },
    },
  });
  await page.getByTestId("turn-action-bar").waitFor();
  await page.getByTestId("ti4-board-svg").waitFor();
  return game;
}

export const barBox = async (page: Page) => (await page.getByTestId("turn-action-bar").boundingBox())!;

/** The floating Players / Events buttons (whatever sits at the bottom edge outside the bar). */
export async function assertFloatingButtonsClear(page: Page) {
  const bar = await barBox(page);
  const boxes = await page.evaluate(() =>
    Array.from(document.querySelectorAll<HTMLElement>("button, [role=button]"))
      .filter((el) => /^(players|events)\b/i.test((el.getAttribute("aria-label") ?? el.textContent ?? "").trim()))
      .map((el) => {
        const r = el.getBoundingClientRect();
        return { label: el.getAttribute("aria-label") ?? el.textContent, top: r.top, bottom: r.bottom, h: r.height, w: r.width };
      }),
  );
  for (const b of boxes) {
    if (b.h === 0) continue;
    const overlaps = b.top < bar.y + bar.height && b.bottom > bar.y;
    expect(overlaps, `${b.label} is covered by the action pane`).toBe(false);
  }
  return boxes;
}
