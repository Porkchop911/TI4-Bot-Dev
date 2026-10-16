import type { Page } from "@playwright/test";
import type { ObjectiveProgressView, PlayerView } from "../../../src/protocol/types";
import { openMockedGame, type MockChoice } from "../_shared/mockGame";
import { opponent, playerWithHand, withPlayer } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";

export const SIZES = {
  desktop: { width: 1440, height: 900 },
  "390x844": { width: 390, height: 844 },
} as const;
export type SizeName = keyof typeof SIZES;

const third = withPlayer(opponent, { id: "seat_c", faction: "jolnar", strategy_cards: [] });
const fourth = withPlayer(opponent, { id: "seat_d", faction: "xxcha", strategy_cards: [] });

/**
 * Planet counts per seat for each Push Boundaries state. Everyone neighbours everyone except
 * seat_c and seat_d, so the viewer has three neighbours and each rival two.
 */
export const PUSH_STATES = {
  "ahead-2": { me: 12, other_seat: 10, seat_c: 9, seat_d: 13 },
  "ahead-1": { me: 12, other_seat: 10, seat_c: 13, seat_d: 12 },
  tied: { me: 12, other_seat: 12, seat_c: 12, seat_d: 14 },
} as const;
export type PushState = keyof typeof PUSH_STATES;

/** Systems that count for Learn the Secrets of the Cosmos (gallery tiles beside an anomaly). */
export const LSC_SYSTEMS = ["26", "25", "28"];

function pushProgress(
  me: string,
  counts: Record<string, number>,
): Record<string, ObjectiveProgressView> {
  const seats = [me, "other_seat", "seat_c", "seat_d"];
  const key = (seat: string) => (seat === me ? "me" : seat);
  const neighbours = (seat: string) =>
    seats.filter(
      (other) =>
        other !== seat &&
        !((seat === "seat_c" && other === "seat_d") || (seat === "seat_d" && other === "seat_c")),
    );
  return Object.fromEntries(
    seats.map((seat) => {
      const mine = counts[key(seat)];
      const rows = neighbours(seat).map((other) => {
        const theirs = counts[key(other)];
        return { player: other, mine, theirs, ahead: mine > theirs };
      });
      const have = rows.filter((row) => row.ahead).length;
      return [seat, { have, threshold: 2, satisfied: have >= 2, neighbours: rows }];
    }),
  );
}

export function scene(push: PushState, lscCount: number) {
  const hand = playerWithHand({
    secret_objectives_count: 2,
    held_secret_objectives: ["lsc", "ctr"],
    secret_objective_progress: {
      lsc: {
        have: lscCount,
        threshold: 3,
        satisfied: lscCount >= 3,
        systems: LSC_SYSTEMS.slice(0, lscCount),
      },
      ctr: { have: 4, threshold: 6, satisfied: false },
    },
  });
  const players: PlayerView[] = [hand, opponent, third, fourth];
  const progress = pushProgress(hand.id, PUSH_STATES[push]);
  const table = {
    revealed_objectives: ["corner", "push_boundaries"],
    scored_objectives: {},
    objective_progress: Object.fromEntries(
      players.map((player, index) => [
        player.id,
        {
          corner: { have: [3, 2, 1, 4][index], threshold: 4, satisfied: index === 3 },
          push_boundaries: progress[player.id],
        },
      ]),
    ),
  };
  return { hand, players, table };
}

export async function openScene(page: Page, size: SizeName, push: PushState, lscCount: number) {
  const { players, table } = scene(push, lscCount);
  await page.setViewportSize(SIZES[size]);
  await openMockedGame(page, {
    phase: "action",
    board: galleryBoard,
    players,
    table,
    choice: {
      prompt: "action phase",
      player: opponent.id,
      options: [{ id: "pass", label: "pass", kind: "action" }],
    } as MockChoice,
  });
  await page.getByTestId("ti4-board-svg").waitFor();
  await page.waitForTimeout(400);
}

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
