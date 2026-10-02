import { test, expect, type APIRequestContext, type Page } from "@playwright/test";
import { createStartedGame, openPlayerGame } from "./lobbyHelpers";
import type { InitialSnapshotMsg } from "../src/protocol/types";

const backend = `http://127.0.0.1:${process.env.TI4_E2E_BACKEND_PORT ?? "8080"}`;

async function snapshot(
  request: APIRequestContext,
  gameId: string,
  session: string,
): Promise<InitialSnapshotMsg> {
  const response = await request.get(`${backend}/api/games/${gameId}/snapshot`, {
    headers: { "x-ti4-player-session": session },
  });
  expect(response.ok()).toBe(true);
  return response.json();
}

async function choiceAt(
  request: APIRequestContext,
  gameId: string,
  session: string,
  subtype: string,
) {
  await expect
    .poll(
      async () =>
        (await snapshot(request, gameId, session)).pending_choice?.choice.context?.subtype,
    )
    .toBe(subtype);
  return snapshot(request, gameId, session);
}

async function choose(page: Page, optionId: string) {
  await page.locator(`[data-testid="choice-option"][data-option-id="${optionId}"]`).click();
  await page.getByTestId("submit-choice-button").click();
}

test("a real engine advances after an empty tactical movement", async ({ browser, request }) => {
  const { gameId, players } = await createStartedGame(request, 3, 42);
  const pages: Page[] = [];
  for (const player of players) {
    const page = await browser.newPage();
    pages.push(page);
    await openPlayerGame(page, gameId, player.session);
  }

  // Drive only draft decisions; the server can broadcast a phase transition between them.
  for (let pick = 0; pick < 8; pick++) {
    await expect
      .poll(async () => {
        const state = await snapshot(request, gameId, players[0].session);
        return (
          state.view.phase.toLowerCase() === "action" ||
          state.turn_status.kind === "waiting_for_decision"
        );
      })
      .toBe(true);
    const status = await snapshot(request, gameId, players[0].session);
    if (status.view.phase.toLowerCase() === "action") break;
    expect(status.turn_status.kind, `draft pick ${pick}`).toBe("waiting_for_decision");
    const seat = players.findIndex(
      (player) =>
        player.id ===
        (status.turn_status.kind === "waiting_for_decision" ? status.turn_status.seat : ""),
    );
    expect(seat).toBeGreaterThanOrEqual(0);
    const current = await choiceAt(request, gameId, players[seat].session, "draft_strategy_card");
    await choose(pages[seat], current.pending_choice!.choice.options[0].id);
  }

  const actor = players[0];
  const page = pages[0];
  await expect
    .poll(async () =>
      (await snapshot(request, gameId, actor.session)).pending_choice?.choice.options.some(
        (option) => option.id === "tactical",
      ),
    )
    .toBe(true);
  let current = await snapshot(request, gameId, actor.session);
  expect(current.view.phase.toLowerCase()).toBe("action");
  expect(current.pending_choice?.choice.player).toBe(actor.id);
  const tactical = current.pending_choice!.choice.options.find(
    (option) => option.id === "tactical",
  );
  expect(tactical).toBeDefined();
  await choose(page, tactical!.id);

  current = await choiceAt(request, gameId, actor.session, "activate_system");
  expect(current.pending_choice?.choice.context?.subtype).toBe("activate_system");
  const options = current.pending_choice!.choice.options;
  const positions = new Map(current.galaxy_layout.placements.map((p) => [p.system_id, p]));
  const origins = Object.values(current.view.board.systems)
    .filter((system) => system.units.some((unit) => unit.owner === actor.id && !unit.planet))
    .map((system) => positions.get(system.system_id))
    .filter((p) => p !== undefined);
  expect(origins.length).toBeGreaterThan(0);
  // Hex distance > 3 exceeds ordinary ship movement; check the actual next legal choice.
  const distant = options.find((option) => {
    const target = positions.get(option.id);
    return (
      target &&
      origins.every(
        (origin) =>
          (Math.abs(origin.q - target.q) +
            Math.abs(origin.r - target.r) +
            Math.abs(origin.q + origin.r - target.q - target.r)) /
            2 >
          3,
      )
    );
  });
  expect(distant, "seed 42 must offer a distant activation").toBeDefined();
  await page.getByTestId(`system-hex-${distant!.id}`).click();
  await page.getByTestId("confirm-activation-btn").click();

  current = await choiceAt(request, gameId, actor.session, "movement_step");
  expect(current.pending_choice?.choice.context?.subtype).toBe("movement_step");
  expect(current.pending_choice?.choice.options.map((option) => option.id)).toEqual([
    "done_moving",
  ]);
  expect(current.pending_choice?.choice.options[0].kind).toBe("decline");
  const before = current.game_version;
  const beforeStatus = current.turn_status;
  await expect(page.getByTestId("tactical-movement-tray")).toBeVisible();
  await expect(page.getByText("No ships eligible to move into the active system.")).toBeVisible();
  await page.getByTestId("commit-moves-btn").click();
  await expect
    .poll(async () => {
      const state = await snapshot(request, gameId, actor.session);
      return (
        state.game_version > before &&
        state.pending_choice?.choice.context?.subtype !== "movement_step" &&
        JSON.stringify(state.turn_status) !== JSON.stringify(beforeStatus)
      );
    })
    .toBe(true);
  await expect
    .poll(async () => Number((await page.getByTestId("game-version").innerText()).slice(1)))
    .toBeGreaterThan(before);
  await expect(page.getByTestId("tactical-movement-tray")).toHaveCount(0);
  await expect(page.getByRole("alert")).toHaveCount(0);
});
