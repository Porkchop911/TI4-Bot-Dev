import type { Page, TestInfo } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { actor, galleryBoard } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

export interface Planet {
  id: string;
  label: string;
  resources: number;
  influence: number;
  traits?: string[];
  attachments?: string[];
}

/** Opens the landing tray for the given planets of system 18 (synthetic board, not an engine state). */
export async function openLanding(page: Page, planets: Planet[]) {
  const base = galleryBoard.systems["18"];
  const board = {
    ...galleryBoard,
    map_tiles: galleryBoard.map_tiles?.map((tile) =>
      tile.system_id === "18"
        ? {
            ...tile,
            planets: planets.map(({ id, label, resources, influence, traits }) => ({
              id,
              label,
              resources,
              influence,
              ...(traits ? { traits } : {}),
            })),
          }
        : tile,
    ),
    systems: {
      ...galleryBoard.systems,
      "18": {
        ...base,
        planets: Object.fromEntries(
          planets.map((p) => [p.id, { planet_id: p.id, exhausted: false, attachments: p.attachments }]),
        ),
      },
    },
    invasion: {
      system_id: "18",
      invasion_seq: 1,
      invader: actor,
      phase: "landing" as const,
      planets: planets.map((p) => p.id),
      current_planet: null,
      defender: null,
      ground_round: 0,
      odds_context: {},
    },
  };
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    board,
    choice: {
      prompt: "Land ground forces",
      context: { subtype: "commit_ground_forces", target: { System: "18" } },
      options: [
        ...planets.map((p) => ({
          id: `land|${p.id}|infantry`,
          label: `Land infantry on ${p.label}`,
          kind: "land",
          payload: { planet: p.id, unit: "infantry" },
        })),
        { id: "done_landing", label: "Done landing", kind: "decline" },
      ],
    },
  });
  await page.getByTestId("invasion-planet-values").first().waitFor();
}

export async function capture(page: Page, testInfo: TestInfo, name: string) {
  await shot(page, testInfo, name, { of: page.locator(".invasion-landing-body"), pad: 12 });
}
