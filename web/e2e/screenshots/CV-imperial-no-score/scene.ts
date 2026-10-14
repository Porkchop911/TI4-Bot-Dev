import type { Page } from "@playwright/test";
import type { BoardView, GameEvent, PlayerView, TableView } from "../../../src/protocol/types";
import { openMockedGame, type MockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand, withPlayer } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";

export const SIZES = {
  desktop: { width: 1440, height: 900 },
  "390x844": { width: 390, height: 844 },
} as const;
export type SizeName = keyof typeof SIZES;

// Game 8261fb3f, round 5: the viewer is the Xxcha; Letnev has just landed on Archon Ren.
const xxcha = playerWithHand({ faction: "xxcha", victory_points: 7, strategy_cards: ["pok8imperial"] });
const letnev = withPlayer(opponent, { faction: "letnev", victory_points: 6 });
const sol = withPlayer(opponent, { id: "seat_c", faction: "sol", victory_points: 7, strategy_cards: [] });
export const players: PlayerView[] = [xxcha, letnev, sol];
export const viewer = xxcha.id;

/** Archon Ren held by `archonRen`, Archon Tau by the viewer, as system 14 on the board. */
export function board(archonRen: string): BoardView {
  return {
    ...galleryBoard,
    systems: {
      ...galleryBoard.systems,
      "14": {
        system_id: "14",
        command_tokens: [],
        units: [],
        planets: {
          archonren: { planet_id: "archonren", controlled_by: archonRen, exhausted: true },
          archontau: { planet_id: "archontau", controlled_by: viewer, exhausted: false },
        },
      },
    },
  };
}

export type Variant = "home-lost" | "none-met" | "home-held";

/** The revealed objectives of that game; push / unify progress as each variant needs it. */
export function table(variant: Variant): Partial<TableView> {
  const met = variant !== "none-met";
  const push = met
    ? { have: 3, threshold: 2, satisfied: true }
    : { have: 1, threshold: 2, satisfied: false };
  const unify = met
    ? { have: 6, threshold: 6, satisfied: true }
    : { have: 5, threshold: 6, satisfied: false };
  const neighbours = met
    ? [
        { player: letnev.id, mine: 14, theirs: 11, ahead: true },
        { player: sol.id, mine: 14, theirs: 12, ahead: true },
      ]
    : [
        { player: letnev.id, mine: 11, theirs: 11, ahead: false },
        { player: sol.id, mine: 11, theirs: 10, ahead: true },
      ];
  return {
    revealed_objectives: ["sway_council", "corner", "lost_outposts", "make_history", "push_boundaries", "unify_colonies"],
    scored_objectives: {
      [viewer]: ["corner", "lost_outposts", "make_history", "sway_council"],
      [sol.id]: ["corner"],
    },
    objective_progress: {
      [viewer]: { push_boundaries: { ...push, neighbours }, unify_colonies: unify },
      [letnev.id]: {
        corner: { have: 3, threshold: 4, satisfied: false },
        push_boundaries: { have: 0, threshold: 2, satisfied: false },
        unify_colonies: { have: 4, threshold: 6, satisfied: false },
      },
      [sol.id]: {
        push_boundaries: { have: 1, threshold: 2, satisfied: false },
        unify_colonies: { have: 5, threshold: 6, satisfied: false },
      },
    },
    ...(variant === "home-lost" ? { home_system_lost: { [viewer]: ["archonren"] } } : {}),
  };
}

export async function openScene(page: Page, size: SizeName, variant: Variant): Promise<MockedGame> {
  await page.setViewportSize(SIZES[size]);
  const game = await openMockedGame(page, {
    phase: "action",
    round: 5,
    board: board(variant === "home-lost" ? letnev.id : viewer),
    players,
    table: table(variant),
    choice: {
      prompt: "spend a strategy token to draw a secret objective",
      player: letnev.id,
      options: [
        { id: "no", label: "no", kind: "decline" },
        { id: "yes", label: "yes", kind: "action" },
      ],
    },
  });
  await page.getByTestId("ti4-board-svg").waitFor();
  await page.waitForTimeout(400);
  return game;
}

/** The log entry the server writes when the viewer plays Imperial. */
export const playedImperial = (id: string): GameEvent =>
  ({
    id,
    timestamp: "05:35:08",
    visibility: { visibility: "public" },
    event: { kind: "decision_resolved" },
    actor: viewer,
    round: 5,
    phase: "action",
    action_type: "strategic",
    action_actor: viewer,
    stage: "action selection",
    detail: `${viewer} played Imperial`,
  }) as GameEvent;

export async function openObjectives(page: Page, size: SizeName) {
  await page
    .getByTestId(size === "desktop" ? "objectives-modal-button" : "objectives-mobile-button")
    .click();
  await page.getByTestId("objectives-modal").waitFor();
}

export async function noHorizontalPageScroll(page: Page) {
  return page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
}
