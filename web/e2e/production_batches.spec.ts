import { expect, test, type APIRequestContext } from "@playwright/test";
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

async function snapshot(request: APIRequestContext, game: string, session: string) {
  const response = await request.get(`${backend}/api/games/${game}/snapshot`, {
    headers: { "x-ti4-player-session": session },
  });
  expect(response.ok(), await response.text()).toBe(true);
  return (await response.json()) as InitialSnapshotMsg;
}

test("free production queues multiple builds without interrupting", async ({ page, request }) => {
  test.setTimeout(90_000);
  const {
    game_id: game,
    player_id: player,
    player_session: session,
  } = await launch(request, "production_batch");
  const before = await snapshot(request, game, session);
  expect(before.pending_choice?.choice.context?.subtype).toBe("produce_unit");
  const original = before.view.board.systems["01"].units.filter(
    (unit) => unit.owner === player && unit.unit_type === "sol_infantry",
  ).length;
  await openPlayerGame(page, game, session);
  const add = page.getByTestId("produce-unit-btn-build|sol_infantry|2");
  await expect(add).toBeVisible();
  await add.click();
  await add.click();
  const builds: unknown[] = [];
  page.on("request", (sent) => {
    if (sent.url().endsWith(`/api/games/${game}/batches`)) {
      const plan = sent.postDataJSON().plan;
      if (plan.kind === "production") builds.push(plan.steps);
    }
  });
  await page.getByRole("button", { name: "Confirm builds" }).click();
  await expect
    .poll(async () => {
      const state = await snapshot(request, game, session);
      return state.view.board.systems["01"].units.filter(
        (unit) => unit.owner === player && unit.unit_type === "sol_infantry",
      ).length;
    })
    .toBe(original + 4);
  expect(builds).toEqual([
    [{ kind: "produce", unit: "sol_infantry", count: 2 }],
    [{ kind: "produce", unit: "sol_infantry", count: 2 }],
  ]);
  await expect(page.getByTestId("production-error-banner")).toHaveCount(0);
});

test("exhausting multiple planets in one payment does not interrupt the workflow", async ({
  page,
  request,
}) => {
  test.setTimeout(90_000);
  const { game_id: game, player_session: session } = await launch(
    request,
    "production_payment_batch",
  );
  const production = await snapshot(request, game, session);
  expect(production.pending_choice?.choice.options.map((option) => option.id)).toContain(
    "build|dreadnought|1",
  );
  await openPlayerGame(page, game, session);
  await page.getByTestId("produce-unit-btn-build|dreadnought|1").click();
  const build = page.waitForResponse((response) =>
    response.url().endsWith(`/api/games/${game}/batches`),
  );
  await page.getByRole("button", { name: "Confirm builds" }).click();
  const buildResponse = await build;
  expect(buildResponse.ok(), await buildResponse.text()).toBe(true);
  await expect(page.getByTestId("confirm-payment-btn")).toBeVisible();
  const state = await snapshot(request, game, session);
  const options = state.pending_choice!.choice.options.filter((option) =>
    option.id.startsWith("exhaust|"),
  );
  const other = options.find(
    (option) => option.id !== "exhaust|jord" && Number(option.payload?.worth) < 4,
  );
  expect(other, "scenario must offer a smaller planet").toBeDefined();
  await page.getByTestId(`planet-card-${other!.id}`).click();
  await page.getByTestId("planet-card-exhaust|jord").click();
  const batch = page.waitForResponse((response) =>
    response.url().endsWith(`/api/games/${game}/batches`),
  );
  await page.getByTestId("confirm-payment-btn").click();
  const response = await batch;
  expect(response.request().postDataJSON().plan.steps).toEqual([
    { kind: "exhaust", planet: other!.id.slice("exhaust|".length) },
    { kind: "exhaust", planet: "jord" },
  ]);
  expect(response.ok(), `batch response ${response.status()}: ${await response.text()}`).toBe(true);
  await expect(page.getByTestId("payment-error-banner")).toHaveCount(0);
  const after = await snapshot(request, game, session);
  expect(after.pending_choice?.choice.context?.subtype).not.toBe("pay_resources");
  for (const planet of [other!.id.slice("exhaust|".length), "jord"]) {
    const system = Object.values(after.view.board.systems).find((system) => system.planets[planet]);
    expect(system?.planets[planet].exhausted, `${planet} was spent`).toBe(true);
  }
});

test("paid infantry builds pause for payment and resume without a second-step rejection", async ({
  page,
  request,
}) => {
  test.setTimeout(90_000);
  const {
    game_id: game,
    player_id: player,
    player_session: session,
  } = await launch(request, "production_payment_batch");
  const before = await snapshot(request, game, session);
  const infantry = () =>
    snapshot(request, game, session).then(
      (state) =>
        state.view.board.systems["01"].units.filter(
          (unit) => unit.owner === player && unit.unit_type === "sol_infantry",
        ).length,
    );
  const original = await infantry();
  const rejected: string[] = [];
  const builds: unknown[] = [];
  page.on("request", (sent) => {
    if (sent.url().endsWith(`/api/games/${game}/batches`)) {
      const plan = sent.postDataJSON().plan;
      if (plan.kind === "production") builds.push(plan.steps);
    }
  });
  page.on("response", async (response) => {
    if (response.url().endsWith(`/api/games/${game}/batches`) && !response.ok())
      rejected.push(await response.text());
  });
  await openPlayerGame(page, game, session);
  const add = page.getByTestId("produce-unit-btn-build|sol_infantry|2");
  await add.click();
  await add.click();
  await page.getByRole("button", { name: "Confirm builds" }).click();
  await expect(page.getByTestId("confirm-payment-btn")).toBeVisible();
  await expect(page.getByTestId("payment-drawer")).toContainText("Resources");
  expect(before.pending_choice?.choice.context?.subtype).toBe("produce_unit");
  await page.getByTestId("planet-card-exhaust|jord").click();
  await page.getByTestId("confirm-payment-btn").click();
  await expect.poll(infantry, { timeout: 15_000 }).toBe(original + 4);
  expect(builds).toEqual([
    [{ kind: "produce", unit: "sol_infantry", count: 2 }],
    [{ kind: "produce", unit: "sol_infantry", count: 2 }],
  ]);
  expect(rejected).toEqual([]);
  await expect(page.getByTestId("production-error-banner")).toHaveCount(0);
});

test("payment does not submit a second exhaust after the engine auto-spends Wren Terra", async ({
  page,
  request,
}) => {
  test.setTimeout(90_000);
  const { game_id: game, player_session: session } = await launch(
    request,
    "production_payment_autospend",
  );
  await openPlayerGame(page, game, session);
  await page.getByTestId("produce-unit-btn-build|dreadnought|1").click();
  const build = page.waitForResponse((response) =>
    response.url().endsWith(`/api/games/${game}/batches`),
  );
  await page.getByRole("button", { name: "Confirm builds" }).click();
  expect((await build).ok()).toBe(true);
  await expect(page.getByTestId("confirm-payment-btn")).toBeVisible();
  const payment = await snapshot(request, game, session);
  expect(payment.pending_choice?.choice.options.map((option) => option.id)).toEqual([
    "exhaust|jord",
    "exhaust|wrenterra",
  ]);
  await page.getByTestId("planet-card-exhaust|wrenterra").click();
  await page.getByTestId("planet-card-exhaust|jord").click();
  const batch = page.waitForResponse((response) =>
    response.url().endsWith(`/api/games/${game}/batches`),
  );
  await page.getByTestId("confirm-payment-btn").click();
  const response = await batch;
  expect(response.request().postDataJSON().plan.steps).toEqual([
    { kind: "exhaust", planet: "wrenterra" },
  ]);
  expect(response.ok(), `batch response ${response.status()}: ${await response.text()}`).toBe(true);
  await expect(page.getByTestId("payment-error-banner")).toHaveCount(0);
  const after = await snapshot(request, game, session);
  expect(after.view.board.systems["10"].planets.wrenterra.exhausted).toBe(true);
  expect(after.view.board.systems["01"].planets.jord.exhausted).toBe(true);
});

test("payment does not exhaust Wren Terra after Jord already settles the bill", async ({
  page,
  request,
}) => {
  test.setTimeout(90_000);
  const { game_id: game, player_session: session } = await launch(
    request,
    "production_payment_autospend",
  );
  await openPlayerGame(page, game, session);
  await page.getByTestId("produce-unit-btn-build|dreadnought|1").click();
  await page.getByRole("button", { name: "Confirm builds" }).click();
  await expect(page.getByTestId("confirm-payment-btn")).toBeVisible();
  await page.getByTestId("planet-card-exhaust|jord").click();
  await page.getByTestId("planet-card-exhaust|wrenterra").click();
  const batch = page.waitForResponse(
    (response) =>
      response.url().endsWith(`/api/games/${game}/batches`) &&
      response.request().postDataJSON()?.plan?.kind === "payment",
  );
  await page.getByTestId("confirm-payment-btn").click();
  const response = await batch;
  expect(response.request().postDataJSON().plan.steps).toEqual([
    { kind: "exhaust", planet: "jord" },
  ]);
  expect(response.ok(), `batch response ${response.status()}: ${await response.text()}`).toBe(true);
  await expect(page.getByTestId("payment-error-banner")).toHaveCount(0);
  const after = await snapshot(request, game, session);
  expect(after.view.board.systems["01"].planets.jord.exhausted).toBe(true);
  expect(after.view.board.systems["10"].planets.wrenterra.exhausted).toBe(false);
});
