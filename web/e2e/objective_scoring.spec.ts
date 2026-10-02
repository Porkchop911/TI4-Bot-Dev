import { test, expect, type APIRequestContext } from "@playwright/test";
import { openPlayerGame } from "./lobbyHelpers";
import type { InitialSnapshotMsg } from "../src/protocol/types";

const backend = `http://127.0.0.1:${process.env.TI4_E2E_BACKEND_PORT ?? "8080"}`;

async function launch(request: APIRequestContext, scenario: string) {
  const response = await request.post(`${backend}/api/dev/scenarios/launch`, {
    data: { scenario_id: scenario, seed: 42 },
  });
  expect(response.ok(), await response.text()).toBe(true);
  return (await response.json()) as {
    game_id: string;
    player_id: string;
    player_session: string;
  };
}

async function snapshot(request: APIRequestContext, gameId: string, session: string) {
  const response = await request.get(`${backend}/api/games/${gameId}/snapshot`, {
    headers: { "x-ti4-player-session": session },
  });
  expect(response.ok(), await response.text()).toBe(true);
  return (await response.json()) as InitialSnapshotMsg;
}

test.describe("Objective Scoring Dev Scenarios", () => {
  test("scores an objective during Status Phase out of multiple unscored candidates using the objectives matrix modal", async ({
    page,
    request,
  }) => {
    test.setTimeout(60_000);

    const {
      game_id: game,
      player_id: player,
      player_session: session,
    } = await launch(request, "score_objective_status");

    const initial = await snapshot(request, game, session);
    expect(initial.pending_choice?.choice.context?.subtype).toBe("score_objective");

    await openPlayerGame(page, game, session);

    // Dedicated Objectives Matrix modal should open for objective scoring instead of default pending-choice-dialog
    const matrixModal = page.getByTestId("objectives-modal");
    await expect(matrixModal).toBeVisible();
    await expect(page.getByTestId("pending-choice-dialog")).not.toBeVisible();
    await expect(page.getByTestId("objectives-scoring-footer")).toBeVisible();

    // Verify multiple unscored candidates are offered, plus decline
    const leadOption = page.locator('[data-testid="choice-option"][data-option-id="lead"]');
    const tradeRoutesOption = page.locator(
      '[data-testid="choice-option"][data-option-id="trade_routes"]',
    );
    const declineOption = page.locator('[data-testid="choice-option"][data-option-id="decline"]');
    const cornerOption = page.locator('[data-testid="choice-option"][data-option-id="corner"]');

    await expect(leadOption).toBeVisible();
    await expect(tradeRoutesOption).toBeVisible();
    await expect(declineOption).toBeVisible();
    // "corner" (Corner the Market) is not satisfied, so should not be an actionable choice option
    await expect(cornerOption).toHaveCount(0);

    // Choose Negotiate Trade Routes
    await tradeRoutesOption.click();

    // Submit choice from the modal footer
    const submitBtn = page.getByTestId("submit-choice-button");
    await expect(submitBtn).toBeEnabled();
    await submitBtn.click();

    // Wait for Status Phase to advance to the next step (gain command token)
    await expect(page.getByTestId("choice-prompt")).toContainText(
      "gain a command token into which pool",
    );

    // Minimize subsequent choice to clear modal backdrop and view board/tables
    const minimizeBtn = page.getByTestId("minimize-choice-button");
    await expect(minimizeBtn).toBeVisible();
    await minimizeBtn.click();
    await expect(page.getByTestId("minimized-choice-banner")).toBeVisible();

    // Open Objectives matrix modal in inspection mode
    const objectivesBtn = page.getByTestId("objectives-modal-button");
    await expect(objectivesBtn).toBeVisible();
    await objectivesBtn.click();

    await expect(matrixModal).toBeVisible();

    // Check that Negotiate Trade Routes is marked as Scored for Sol
    const tradeRoutesScoredCell = page.getByTestId(`status-scored-trade_routes-${player}`);
    await expect(tradeRoutesScoredCell).toBeVisible();
    await expect(tradeRoutesScoredCell).toContainText("Scored");

    // Lead From the Front should still show Ready (not scored)
    const leadReadyCell = page.getByTestId(`status-ready-lead-${player}`);
    await expect(leadReadyCell).toBeVisible();

    // Player header should show 3 VP (started with 2 VP)
    const playerCol = page.getByTestId(`player-col-header-${player}`);
    await expect(playerCol).toContainText("3 VP");
  });

  test("scores an objective using Imperial primary ability with the objectives matrix modal", async ({
    page,
    request,
  }) => {
    test.setTimeout(60_000);

    const {
      game_id: game,
      player_id: player,
      player_session: session,
    } = await launch(request, "score_objective_imperial");

    const initial = await snapshot(request, game, session);
    expect(initial.pending_choice?.choice.context?.subtype).toBe("imperial_score_objective");

    await openPlayerGame(page, game, session);

    // Dedicated Objectives Matrix modal should open for imperial objective scoring
    const matrixModal = page.getByTestId("objectives-modal");
    await expect(matrixModal).toBeVisible();
    await expect(page.getByTestId("pending-choice-dialog")).not.toBeVisible();
    await expect(page.getByTestId("objectives-scoring-footer")).toBeVisible();

    // Verify multiple unscored candidates are offered
    const leadOption = page.locator('[data-testid="choice-option"][data-option-id="lead"]');
    const tradeRoutesOption = page.locator(
      '[data-testid="choice-option"][data-option-id="trade_routes"]',
    );
    await expect(leadOption).toBeVisible();
    await expect(tradeRoutesOption).toBeVisible();

    // Choose Lead From the Front
    await leadOption.click();

    // Submit choice from the modal footer
    const submitBtn = page.getByTestId("submit-choice-button");
    await expect(submitBtn).toBeEnabled();
    await submitBtn.click();

    // Imperial completes the action and offers the end-of-turn decision.
    await expect(page.getByTestId("choice-prompt")).toContainText("end your turn");

    // Minimize subsequent choice to clear modal backdrop and view board/tables
    const minimizeBtn = page.getByTestId("minimize-choice-button");
    await expect(minimizeBtn).toBeVisible();
    await minimizeBtn.click();
    await expect(page.getByTestId("minimized-choice-banner")).toBeVisible();

    // Open Objectives matrix modal in inspection mode
    const objectivesBtn = page.getByTestId("objectives-modal-button");
    await expect(objectivesBtn).toBeVisible();
    await objectivesBtn.click();

    await expect(matrixModal).toBeVisible();

    // Check that Lead From the Front is marked as Scored for Sol
    const leadScoredCell = page.getByTestId(`status-scored-lead-${player}`);
    await expect(leadScoredCell).toBeVisible();
    await expect(leadScoredCell).toContainText("Scored");

    // Negotiate Trade Routes should still show Ready (not scored)
    const tradeRoutesReadyCell = page.getByTestId(`status-ready-trade_routes-${player}`);
    await expect(tradeRoutesReadyCell).toBeVisible();

    // Player header should show 3 VP (started with 2 VP)
    const playerCol = page.getByTestId(`player-col-header-${player}`);
    await expect(playerCol).toContainText("3 VP");
  });
});
