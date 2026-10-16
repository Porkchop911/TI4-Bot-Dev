import type { Page } from "@playwright/test";
import { openMockedGame, type MockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent, actor } from "../_shared/players";
import { boardWithExhausted } from "../AH-secondary-prep-real-ui/prep";

/** Which side of the change a run is: `before` is taken on the commit with one question per planet. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

/** The viewer's exhausted planets: Jord (2/2), Wellon and Tarmann, all three controlled by the viewer. */
export const EXHAUSTED = ["jord", "wellon", "tarmann"] as const;
export const board = boardWithExhausted(...EXHAUSTED);

/** Diplomacy's "ready which planet" as the engine asks it (once per planet; the engine owes two here). */
export const readyChoice = (planets: readonly string[] = EXHAUSTED, nonce = "shots-bo-ready") => ({
  prompt: "ready which planet",
  nonce,
  context: { subtype: "ready_planet", source: { Rule: "34.2" } },
  options: planets.map((planet) => ({
    id: planet,
    kind: "ready",
    label: `ready ${planet}`,
    payload: { planet },
  })),
});

/** The primary or secondary: the question is the same, so the live scene is one. */
export async function openReady(page: Page, options: { seat?: string; planets?: readonly string[] } = {}): Promise<MockedGame> {
  const seat = options.seat ?? actor;
  const game = await openMockedGame(page, {
    seat,
    players: [playerWithHand({ strategy_cards: ["pok2diplomacy"] }), opponent],
    board,
    choice: { ...readyChoice(options.planets), player: actor },
  });
  await page
    .getByTestId(prefix === "before" || seat !== actor ? "planet-selection-bar" : "ready-planets-bar")
    .waitFor();
  return game;
}

/** Picks planets the way the screen of this side lets you: the old bar one at a time, the new one together. */
export const barId = () => (prefix === "before" ? "planet-selection-bar" : "ready-planets-bar");
