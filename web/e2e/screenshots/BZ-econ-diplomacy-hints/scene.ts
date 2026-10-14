import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";

/** Which side of the change a run is: `before` is taken on the commit without the change. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const SIZES = {
  desktop: { width: 1440, height: 900 },
  "390x844": { width: 390, height: 844 },
} as const;
export type SizeName = keyof typeof SIZES;

const tokens = { tactic: 3, fleet: 3, strategy: 2 };

/** The seat's turn menu with Economic Initiative in its hand; `effects` is the engine's display detail. */
export async function openTurnMenu(page: Page, size: SizeName, withEffects: boolean) {
  await page.setViewportSize(SIZES[size]);
  await openMockedGame(page, {
    board: galleryBoard,
    players: [playerWithHand(), opponent],
    choice: {
      prompt: "action phase",
      nonce: "bz-turn",
      options: [
        { id: "tactical", label: "take a tactical action", kind: "action" },
        { id: "strategic", label: "take your strategic action", kind: "action" },
        { id: "action_card|0", label: "play Economic Initiative", kind: "action" },
        { id: "pass", label: "pass", kind: "action" },
      ],
      details: {
        kind: "turn_menu",
        closing: false,
        tokens,
        partners: [],
        strategy_cards: [{ card: "pok8imperial", used: false, option: "strategic" }],
        ...(withEffects
          ? {
              action_card_effects: {
                "action_card|0": {
                  kind: "ready_cultural",
                  planets: [
                    { id: "lodor", name: "Lodor", trait: "cultural", exhausted: true },
                    { id: "quann", name: "Quann", trait: "cultural", exhausted: false },
                    { id: "corneeq", name: "Corneeq", trait: "cultural", exhausted: true },
                  ],
                },
              },
            }
          : {}),
      },
    },
  });
  await page.getByTestId("turn-action-bar").waitFor();
}

export const READY_HINTS = {
  planets: [
    { id: "lodor", name: "Lodor", resources: 3, influence: 1 },
    { id: "corneeq", name: "Corneeq", resources: 1, influence: 2 },
    { id: "quann", name: "Quann", resources: 2, influence: 1 },
  ],
  spenders: [
    { card: "Leadership", spends: "influence", status: "played" },
    { card: "Technology", spends: "resources", status: "to_come" },
    { card: "Warfare", spends: "resources", status: "not_in_play" },
  ],
};

export async function openSecondary(page: Page, size: SizeName, withHints: boolean) {
  await page.setViewportSize(SIZES[size]);
  await openMockedGame(page, {
    board: galleryBoard,
    players: [playerWithHand(), opponent],
    choice: {
      prompt: "spend a strategic token to ready two planets",
      nonce: "bz-secondary",
      context: { subtype: "strategy_secondary", source: { StrategyCard: { card: "pok2diplomacy", secondary: true } } },
      options: [
        { id: "no", label: "decline", kind: "strategy" },
        { id: "yes", label: "ready", kind: "strategy" },
      ],
      details: {
        kind: "strategy_secondary",
        card: "pok2diplomacy",
        played_by: opponent.id,
        tokens_left: 2,
        costs_token: true,
        ...(withHints ? { ready_hints: READY_HINTS } : {}),
      },
    },
  });
  await page.getByTestId("strategy-secondary-panel").waitFor();
}

export async function openReadyBar(page: Page, size: SizeName, withHints: boolean) {
  await page.setViewportSize(SIZES[size]);
  await openMockedGame(page, {
    board: galleryBoard,
    players: [playerWithHand(), opponent],
    choice: {
      prompt: "ready which planet",
      nonce: "bz-ready",
      context: { subtype: "ready_planet", source: { StrategyCard: { card: "pok2diplomacy", secondary: true } } },
      options: READY_HINTS.planets.map((p) => ({ id: p.id, label: `ready ${p.id}`, kind: "ready" })),
      ...(withHints ? { details: { ready_hints: READY_HINTS } } : {}),
    },
  });
  await page.getByTestId("ready-planets-bar").waitFor();
}
