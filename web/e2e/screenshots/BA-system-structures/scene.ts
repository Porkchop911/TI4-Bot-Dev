import type { Page, TestInfo } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { actor, galleryBoard } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { systemActivationOptions } from "../_shared/fixtures";

/** "before" is captured on the base commit (unified-latest-2026-10-08); the default regenerates the current code. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

type Unit = { unit_type: string; owner: string; planet?: string; damaged: boolean };
const u = (unit_type: string, owner: string, planet?: string, damaged = false): Unit => ({
  unit_type,
  owner,
  ...(planet ? { planet } : {}),
  damaged,
});

/** Synthetic: system 18 with two planets (Jord, Exhausted World) and the given units. */
export async function openInspector(page: Page, units: Unit[], players = [playerWithHand(), opponent]) {
  const board = {
    ...galleryBoard,
    systems: {
      ...galleryBoard.systems,
      "18": { ...galleryBoard.systems["18"], units },
    },
  };
  await openMockedGame(page, {
    players,
    board,
    choice: {
      prompt: "Choose a system to activate",
      context: { subtype: "activate_system" },
      options: systemActivationOptions,
    },
  });
  await page.getByTestId("system-hex-18").click();
  await page.getByTestId("system-inspector").waitFor();
}

export const mine = (planet: string, type: string) => u(type, actor, planet);
export const theirs = (planet: string, type: string) => u(type, opponent.id, planet);
export const ground = (planet: string) => u("infantry", actor, planet);
export const fleet: Unit[] = [u("dreadnought", actor, undefined, true), u("fighter", actor)];

export async function capture(page: Page, testInfo: TestInfo, name: string) {
  await shot(page, testInfo, `${prefix}-${name}`, { of: page.getByTestId("system-inspector"), pad: 12 });
}
