import type { Page } from "@playwright/test";
import { openMockedGame, type MockChoice } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";

/** Real-looking participant ids, so prose that names them is resolved to nicknames. */
const generated = (hex: string) => `player_${hex.repeat(32)}`;
export const SEAT_NP = generated("ab");
export const SEAT_ALEX = generated("ef");
export const SEAT_CASEY = generated("cd");
/** np (Mahact, the viewer) holds Alex's (Sol) and Casey's (Hacan) command tokens. */
export const nicknames = { [SEAT_NP]: "np", [SEAT_ALEX]: "Alex", [SEAT_CASEY]: "Casey" };

export const players = (held: string[] = [SEAT_ALEX, SEAT_CASEY]) => [
  playerWithHand({ id: SEAT_NP, faction: "mahact", held_foreign_tokens: held }),
  { ...opponent, id: SEAT_ALEX, faction: "sol" },
  { ...opponent, id: SEAT_CASEY, faction: "hacan" },
];

const turnMenu: MockChoice = {
  prompt: "action phase",
  options: [
    { id: "tactical", label: "take a tactical action", kind: "action" },
    { id: "pass", label: "pass", kind: "action" },
  ],
  details: { kind: "turn_menu", closing: false, tokens: { tactic: 3, fleet: 3, strategy: 2 }, strategy_cards: [], partners: [] },
};

export async function open(
  page: Page,
  size: { width: number; height: number },
  choice: MockChoice = turnMenu,
  held?: string[],
) {
  await page.setViewportSize(size);
  return openMockedGame(page, { seat: SEAT_NP, players: players(held), nicknames, choice });
}

/** The engine's waiver label, as `secondary_waivers` words it now (the primary player's id stays raw). */
export const waiverLabel = (primary: string) =>
  `exhaust Jae Mir Kan: use ${primary}'s command token from Mecatol Rex (system 18) instead of a token`;
