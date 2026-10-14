import { test, expect, type APIRequestContext, type Page } from "@playwright/test";
import { createStartedGame, gameSnapshot, openPlayerGame } from "./lobbyHelpers";
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
  expect(response.ok(), `snapshot: ${response.status()}`).toBe(true);
  return response.json();
}

async function startMovement(page: Page, request: APIRequestContext) {
  const launch = await request.post(`${backend}/api/dev/scenarios/launch`, {
    data: { scenario_id: "tactical_action", seed: 42 },
  });
  expect(launch.ok(), `launch scenario: ${launch.status()}`).toBe(true);
  const { game_id: gameId, player_session: session, player_id: playerId } = await launch.json();
  const initial = await snapshot(request, gameId, session);
  const opponent = initial.view.players.find((p) => p.faction?.toLowerCase().includes("hacan"));
  expect(opponent).toBeDefined();
  const border = Object.values(initial.view.board.systems).find(
    (system) =>
      system.system_id !== "16" && system.units.some((unit) => unit.owner === opponent!.id),
  );
  expect(border).toBeDefined();
  const forward = Object.values(initial.view.board.systems).find(
    (system) => system.system_id !== "01" && system.units.some((unit) => unit.owner === playerId),
  );
  expect(forward).toBeDefined();

  await openPlayerGame(page, gameId, session);
  await page.locator('[data-testid="choice-option"][data-option-id="tactical"]').click();
  await page.getByTestId("submit-choice-button").click();
  await page.getByTestId(`system-hex-${border!.system_id}`).click();
  await page.getByTestId("confirm-activation-btn").click();
  const tray = page.getByTestId("tactical-movement-tray");
  await expect(tray).toBeVisible();
  await expect(tray.getByRole("heading", { level: 2 })).toHaveText("Move Units");
  return {
    gameId,
    session,
    playerId,
    opponentId: opponent!.id,
    borderId: border!.system_id,
    forwardId: forward!.system_id,
    tray,
  };
}

test.describe("Tactical fleet rally", () => {
  test("moves ships from both origins and loads staged infantry in a single workflow", async ({
    page,
    request,
  }) => {
    test.setTimeout(90_000);
    const { gameId, session, playerId, opponentId, borderId, forwardId, tray } =
      await startMovement(page, request);
    await expect(page.getByTestId("rally-row-01-sol_carrier")).toBeVisible();
    await expect(page.getByTestId(`rally-row-${forwardId}-cruiser`)).toBeVisible();

    await page.getByTestId("rally-inc-01-sol_carrier").click();
    await page.getByTestId(`rally-inc-${forwardId}-cruiser`).click();
    // The same selection includes a planet-sourced load for the carrier.
    await page.getByTestId("rally-inc-cargo-01-sol_infantry-jord").click();
    await expect(page.getByTestId("cargo-capacity-gauge-01")).toContainText("1 loaded /");
    await expect(page.getByTestId("fleet-supply-gauge")).not.toHaveAttribute(
      "data-warning",
      "true",
    );
    await page.getByTestId("commit-moves-btn").click();
    await expect(page.getByTestId("movement-progress")).toBeVisible();
    await expect(page.getByTestId("cargo-loading-tray")).toHaveCount(0);
    await expect(tray).toHaveCount(0, { timeout: 35_000 });

    await expect
      .poll(
        async () => {
          const current = await snapshot(request, gameId, session);
          const units = current.view.board.systems[borderId]?.units ?? [];
          return {
            carrier: units.some((u) => u.owner === playerId && u.unit_type === "sol_carrier"),
            cruiser: units.some((u) => u.owner === playerId && u.unit_type === "cruiser"),
            infantry: units.some((u) => u.owner === playerId && u.unit_type === "sol_infantry"),
            contested: units.some((u) => u.owner === opponentId),
            moving:
              current.pending_choice?.choice.context?.subtype === "movement_step" ||
              current.pending_choice?.choice.context?.subtype === "load_cargo",
          };
        },
        { timeout: 10_000 },
      )
      .toEqual({ carrier: true, cruiser: true, infantry: true, contested: true, moving: false });
    await expect(page.getByTestId("movement-error-banner")).toHaveCount(0);
  });

  test("advises about excess fleet supply and commits a multi-origin batch", async ({
    page,
    request,
  }) => {
    test.setTimeout(90_000);
    const { gameId, session, playerId, borderId, forwardId, tray } = await startMovement(
      page,
      request,
    );
    // Fleet supply is three; stage four non-fighter ships across both origins.
    let staged = 0;
    for (const origin of ["01", forwardId]) {
      const units = (await snapshot(request, gameId, session)).view.board.systems[origin].units;
      const ships = [
        ...new Set(
          units
            .filter(
              (u) =>
                u.owner === playerId &&
                !u.planet &&
                !["fighter", "infantry", "mech", "pds", "spacedock"].includes(u.unit_type),
            )
            .map((u) => u.unit_type),
        ),
      ];
      for (const type of ships) {
        const increment = page.getByTestId(`rally-inc-${origin}-${type}`);
        while (staged < 4 && (await increment.isEnabled())) {
          await increment.click();
          staged++;
        }
      }
    }
    expect(staged).toBe(4);
    await expect(page.getByTestId("fleet-supply-gauge")).toHaveAttribute("data-warning", "true");
    await expect(page.getByTestId("commit-moves-btn")).toBeEnabled();
    const batchRequest = page.waitForRequest((request) =>
      request.url().endsWith(`/api/games/${gameId}/batches`),
    );
    await page.getByTestId("commit-moves-btn").click();
    const sent = await batchRequest;
    expect(sent.postDataJSON()).toMatchObject({ plan: { kind: "tactical_movement" } });
    expect(sent.postDataJSON().plan.steps).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ kind: "move", origin: "01" }),
        expect.objectContaining({ kind: "move", origin: forwardId }),
      ]),
    );
    const response = await page.waitForResponse((response) => response.request() === sent, {
      timeout: 15_000,
    });
    expect(response.ok(), `batch response ${response.status()}: ${await response.text()}`).toBe(
      true,
    );
    await expect(tray).toHaveCount(0);
    await expect
      .poll(async () => {
        const current = await snapshot(request, gameId, session);
        return (current.view.board.systems[borderId]?.units ?? []).filter(
          (u) => u.owner === playerId && !u.planet && u.unit_type !== "fighter",
        ).length;
      })
      .toBeGreaterThanOrEqual(4);
  });

  test("successfully moves fleet and cargo without workflow interruption when carrier is at full capacity", async ({
    browser,
    request,
  }) => {
    test.setTimeout(90_000);
    const { gameId, players } = await createStartedGame(request, 3, 42);
    const [p1, p2, p3] = players;

    const contextP1 = await browser.newContext();
    const pageP1 = await contextP1.newPage();
    await openPlayerGame(pageP1, gameId, p1.session);

    const contextP2 = await browser.newContext();
    const pageP2 = await contextP2.newPage();
    await openPlayerGame(pageP2, gameId, p2.session);

    const contextP3 = await browser.newContext();
    const pageP3 = await contextP3.newPage();
    await openPlayerGame(pageP3, gameId, p3.session);
    const pages = [pageP1, pageP2, pageP3];

    // Wait for all seats to load turn-status-bar
    await expect(pageP1.locator('[data-testid="turn-status-bar"]')).toBeVisible();

    // Strategy draft: resolve each pick until Action phase
    while (true) {
      const current = await gameSnapshot(request, gameId, p1.session);
      if (current.view.phase.toLowerCase() === "action") break;
      if (current.turn_status.kind !== "waiting_for_decision") break;
      const index = players.findIndex((p) => p.id === current.turn_status.seat);
      const actorPage = pages[index];
      await expect(actorPage.getByTestId("pending-choice-dialog")).toBeVisible();
      await expect(actorPage.getByTestId("submit-choice-button")).toBeEnabled();
      await actorPage.getByTestId("submit-choice-button").click();
      await actorPage.waitForTimeout(300);
    }

    // Now in Action phase, P1 (Sol) is active
    await expect(pageP1.getByTestId("turn-status-bar")).toContainText("action Phase", {
      ignoreCase: true,
    });
    const p1Action = await gameSnapshot(request, gameId, p1.session);
    expect(p1Action.pending_choice?.choice.player).toBe(p1.id);

    // Select tactical action
    await expect(pageP1.getByTestId("pending-choice-dialog")).toBeVisible();
    await pageP1.locator('[data-testid="choice-option"][data-option-id="tactical"]').click();
    await expect(pageP1.getByTestId("submit-choice-button")).toBeEnabled();
    await pageP1.getByTestId("submit-choice-button").click();

    // Select an adjacent system at distance 1 from home system 01 to activate
    await expect
      .poll(async () => {
        const snap = await gameSnapshot(request, gameId, p1.session);
        return snap.pending_choice?.choice.context?.subtype;
      })
      .toBe("activate_system");
    const activateState = await gameSnapshot(request, gameId, p1.session);
    const tiles = activateState.view.board.map_tiles ?? [];
    const homeTile = tiles.find((t) => t.system_id === "01")!;
    const adjacent = tiles.find(
      (t) =>
        t.system_id !== "01" &&
        Math.max(
          Math.abs(t.q - homeTile.q),
          Math.abs(t.r - homeTile.r),
          Math.abs(-t.q - t.r - (-homeTile.q - homeTile.r)),
        ) === 1 &&
        activateState.pending_choice!.choice.options.some((o) => o.id === t.system_id),
    )!;
    const targetSys = adjacent.system_id;
    await pageP1.getByTestId(`system-hex-${targetSys}`).click();
    await pageP1.getByTestId("confirm-activation-btn").click();

    // Tactical movement tray opens
    const tray = pageP1.getByTestId("tactical-movement-tray");
    await expect(tray).toBeVisible();

    // Stage all ships in system 01: 2 carriers and 1 destroyer
    const carrierInc = pageP1.getByTestId("rally-inc-01-sol_carrier");
    await expect(carrierInc).toBeVisible();
    await carrierInc.click();
    await carrierInc.click();

    const destroyerInc = pageP1.getByTestId("rally-inc-01-destroyer");
    await expect(destroyerInc).toBeVisible();
    await destroyerInc.click();

    // Stage all 5 infantry on Jord
    const infantryInc = pageP1.getByTestId("rally-inc-cargo-01-sol_infantry-jord");
    await expect(infantryInc).toBeVisible();
    for (let i = 0; i < 5; i++) {
      await infantryInc.click();
    }

    // Stage 1 fighter from space so Carrier 1 capacity (6) is completely filled
    const fighterCargoInc = pageP1.locator('[data-testid^="rally-inc-cargo-01-fighter"]');
    await expect(fighterCargoInc).toBeVisible();
    await fighterCargoInc.click();

    // Verify fleet supply and capacity are within limits (no warning attributes)
    await expect(pageP1.getByTestId("fleet-supply-gauge")).not.toHaveAttribute(
      "data-warning",
      "true",
    );
    await expect(pageP1.getByTestId("cargo-capacity-gauge-01")).toContainText("6 loaded /");

    // Commit moves and capture batch request/response
    const batchRequestPromise = pageP1.waitForRequest((req) =>
      req.url().endsWith(`/api/games/${gameId}/batches`),
    );
    await pageP1.getByTestId("commit-moves-btn").click();

    const batchReq = await batchRequestPromise;
    const batchRes = await pageP1.waitForResponse((res) => res.request() === batchReq);

    // Confirm that the server accepted the batch without interruption
    expect(batchRes.ok(), `batch response ${batchRes.status()}: ${await batchRes.text()}`).toBe(
      true,
    );

    // Confirm that the tray closes and no error banner appears
    await expect(tray).toHaveCount(0, { timeout: 35_000 });
    await expect(pageP1.getByTestId("movement-error-banner")).toHaveCount(0);

    // Verify units arrived in target system
    await expect
      .poll(async () => {
        const snap = await gameSnapshot(request, gameId, p1.session);
        const units = snap.view.board.systems[targetSys]?.units ?? [];
        return {
          carriers: units.filter((u) => u.owner === p1.id && u.unit_type === "sol_carrier").length,
          destroyers: units.filter((u) => u.owner === p1.id && u.unit_type === "destroyer").length,
          infantry: units.filter((u) => u.owner === p1.id && u.unit_type === "sol_infantry").length,
          fighters: units.filter((u) => u.owner === p1.id && u.unit_type === "fighter").length,
        };
      })
      .toEqual({
        carriers: 2,
        destroyers: 1,
        infantry: 5,
        fighters: 1,
      });

    await contextP1.close();
    await contextP2.close();
    await contextP3.close();
  });
});
