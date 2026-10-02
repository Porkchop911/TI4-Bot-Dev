import { expect, test, type Page, type WebSocketRoute } from "@playwright/test";
import {
  PROTOCOL_VERSION,
  type ClientMessage,
  type InitialSnapshotMsg,
  type LobbyDto,
  type StateUpdateMsg,
} from "../src/protocol/types";
import type { BasketPlan } from "../src/protocol/client";

const gameId = "mocked-decision";
const seat = "p1";
const session = "mock-session";
const nonce = "movement-7";

// Synthetic protocol-shaped decision: this test proves browser/socket behavior, not engine reachability.
const initial: InitialSnapshotMsg & { type: "initial_snapshot" } = {
  type: "initial_snapshot",
  protocol_version: PROTOCOL_VERSION,
  game_id: gameId,
  game_version: 7,
  viewer: { role: "player", seat },
  state: {},
  galaxy_layout: { version: 1, active_sources: [], placements: [] },
  view: {
    round: 1,
    phase: "action",
    speaker: seat,
    seating_order: [seat, "p2"],
    active_player: seat,
    finished: false,
    players: [],
    board: { systems: {} },
    table: {
      revealed_objectives: [],
      scored_objectives: {},
      unclaimed_strategy_cards: [],
      strategy_card_goods: {},
      laws: {},
    },
  },
  turn_status: {
    kind: "waiting_for_decision",
    seat,
    phase: "action",
    round: 1,
    stage: "movement_step",
  },
  pending_choice: {
    nonce,
    choice: {
      player: seat,
      prompt: "Move ships",
      context: { subtype: "movement_step", target: { System: "42" } },
      options: [{ id: "done_moving", kind: "decline", label: "Finish movement" }],
    },
  },
  events: [],
};

const lobby: LobbyDto = {
  game_id: gameId,
  phase: "running",
  lobby_version: 1,
  host_player_id: seat,
  slots: [seat, "p2"].map((id, index) => ({
    slot_id: `slot-${index + 1}`,
    position: index + 1,
    occupant: id,
    nickname: `Player ${index + 1}`,
    ready: true,
    connected: true,
    can_take_over: false,
  })),
};

async function openMockedGame(page: Page, snapshot = initial) {
  // Register both routes before navigation: the HTTP load and socket subscribe can race.
  await page.route(`**/api/games/${gameId}/lobby/join`, (route) =>
    route.fulfill({
      json: {
        player_session: session,
        player: { id: seat },
        lobby,
      },
    }),
  );
  await page.route(`**/api/games/${gameId}/lobby/heartbeat`, (route) =>
    route.fulfill({ json: {} }),
  );
  await page.route(`**/api/games/${gameId}/snapshot`, (route) => route.fulfill({ json: snapshot }));

  let connected!: (connection: { socket: WebSocketRoute; subscribe: ClientMessage }) => void;
  const socketReady = new Promise<{ socket: WebSocketRoute; subscribe: ClientMessage }>(
    (resolve) => {
      connected = resolve;
    },
  );
  await page.routeWebSocket(`**/ws/games/${gameId}`, (socket) => {
    socket.onMessage((data) => {
      const message = JSON.parse(String(data)) as ClientMessage;
      if (message.type === "subscribe") {
        socket.send(JSON.stringify(snapshot));
        connected({ socket, subscribe: message });
      }
    });
  });
  await page.goto("/");
  await page.evaluate(
    ([id, credential]) => sessionStorage.setItem(`ti4.player-session:${id}`, credential),
    [gameId, session],
  );
  await page.goto(`/games/${gameId}`);
  return socketReady;
}

type BatchRequest = {
  request_id: string;
  expected_version: number;
  nonce: string;
  plan: BasketPlan;
};

async function mockBatchSubmission(page: Page, snapshot: typeof initial) {
  const requests: BatchRequest[] = [];
  await page.route(`**/api/games/${gameId}/batches`, (route) => {
    const body = route.request().postDataJSON() as BatchRequest;
    requests.push(body);
    return route.fulfill({
      json: {
        request_id: body.request_id,
        batch_id: "batch-1",
        start_cursor: 0,
        end_cursor: body.plan.steps.length,
        active: true,
        snapshot: {
          ...snapshot,
          game_version: snapshot.game_version + body.plan.steps.length,
          pending_choice: null,
          turn_status: { kind: "active_turn", player: seat, phase: "action", round: 1 },
        },
      },
    });
  });
  return requests;
}

test("activation on the map shows confirmation bar and inspector without generic modal", async ({
  page,
}) => {
  const activation: typeof initial = {
    ...initial,
    view: {
      ...initial.view,
      board: {
        systems: {},
        map_tiles: [
          { system_id: "18", label: "Mecatol Rex", q: 0, r: 0 },
          { system_id: "34", label: "Abyz", q: 1, r: 0 },
        ],
      },
    },
    pending_choice: {
      nonce: "activate-7",
      choice: {
        player: seat,
        prompt: "Activate a system",
        context: { subtype: "activate_system" },
        options: [{ id: "activate|18", kind: "activate", label: "18", payload: { system: "18" } }],
      },
    },
  };
  const { socket } = await openMockedGame(page, activation);
  const submissions: ClientMessage[] = [];
  socket.onMessage((data) => {
    const message = JSON.parse(String(data)) as ClientMessage;
    if (message.type === "submit_choice") submissions.push(message);
  });
  await expect(page.getByTestId("pending-choice-dialog")).toHaveCount(0);
  await expect(page.getByTestId("system-activation-bar")).toBeVisible();
  await page.getByTestId("system-hex-18").click();
  await expect(page.getByTestId("system-inspector")).toBeVisible();
  await page.getByTestId("event-log-toggle").click();
  const inspector = await page.getByTestId("system-inspector").boundingBox();
  const eventLog = await page.locator("#event-log-drawer").boundingBox();
  expect(inspector).not.toBeNull();
  expect(eventLog).not.toBeNull();
  expect(inspector!.y + inspector!.height).toBeLessThanOrEqual(eventLog!.y);
  await page.getByTestId("close-inspector-button").click();
  await expect(page.getByTestId("system-inspector")).toHaveCount(0);
  await page.getByTestId("confirm-activation-btn").click();
  await expect.poll(() => submissions.length).toBe(1);
  expect(submissions[0]).toMatchObject({ option_id: "activate|18", nonce: "activate-7" });
});

test("production builder accepts a real pointer click on a unit", async ({ page }) => {
  const production: typeof initial = {
    ...initial,
    pending_choice: {
      nonce: "produce-7",
      choice: {
        player: seat,
        prompt: "produce in 18 (3 left)",
        context: {
          subtype: "produce_unit",
          target: { System: "18" },
          outstanding: [{ kind: "production_capacity", amount: 3, paid: 0 }],
        },
        options: [
          {
            id: "build|carrier|1",
            kind: "produce",
            label: "produce 1x carrier for 3",
            payload: { unit: "carrier", count: 1, cost: 3, available_resources: 5 },
          },
          { id: "done_producing", kind: "decline", label: "produce nothing further" },
        ],
      },
    },
  };
  const batches = await mockBatchSubmission(page, production);
  const { socket } = await openMockedGame(page, production);
  const submissions: ClientMessage[] = [];
  socket.onMessage((data) => {
    const message = JSON.parse(String(data)) as ClientMessage;
    if (message.type === "submit_choice") submissions.push(message);
  });
  await expect(page.getByTestId("production-builder-drawer")).toBeVisible();
  await expect(page.getByTestId("production-resources-counter")).toHaveText(
    "0 / 5 Resources (5 Left)",
  );
  await expect(page.getByText("1x carrier for 3")).toBeVisible();
  await expect(page.getByText("produce 1x carrier for 3")).toHaveCount(0);
  await page.getByTestId("produce-unit-btn-build|carrier|1").click({ timeout: 3_000 });
  await expect(page.getByTestId("produce-count-build|carrier|1")).toHaveText("1");
  await expect(page.getByTestId("production-capacity-counter")).toHaveText("1 / 3 Units (2 Left)");
  await expect(page.getByTestId("production-resources-counter")).toHaveText(
    "3 / 5 Resources (2 Left)",
  );
  await expect(page.getByTestId("produce-unit-btn-build|carrier|1")).toBeDisabled();
  expect(submissions).toHaveLength(0);
  await page.getByRole("button", { name: "Confirm builds" }).click();
  await expect.poll(() => batches.length).toBe(1);
  expect(batches[0]).toMatchObject({
    expected_version: 7,
    nonce: "produce-7",
    plan: {
      kind: "production",
      destination: "18",
      steps: [{ kind: "produce", unit: "carrier", count: 1 }],
    },
  });
  expect(batches[0].request_id).toBeTruthy();
  expect(submissions).toHaveLength(0);
});

test("production unit grid fits within the modal and reset clears only the draft", async ({
  page,
}) => {
  const production: typeof initial = {
    ...initial,
    pending_choice: {
      nonce: "produce-scroll",
      choice: {
        player: seat,
        prompt: "produce in 18",
        context: {
          subtype: "produce_unit",
          target: { System: "18" },
          outstanding: [{ kind: "production_capacity", amount: 5, paid: 0 }],
        },
        options: [
          ...Array.from({ length: 18 }, (_, i) => ({
            id: `build|unit${i}|1`,
            kind: "produce",
            label: `Unit ${i}`,
            payload: { unit: `unit${i}`, production_spent: 1, cost: 1, available_resources: 12 },
          })),
          { id: "done_producing", kind: "decline", label: "Done" },
        ],
      },
    },
  };
  const { socket } = await openMockedGame(page, production);
  const submissions: ClientMessage[] = [];
  socket.onMessage((data) => {
    const message = JSON.parse(String(data)) as ClientMessage;
    if (message.type === "submit_choice") submissions.push(message);
  });
  const drawer = page.getByTestId("production-builder-drawer");
  const list = page.getByTestId("production-options-grid");
  await expect(drawer).toBeVisible();
  expect((await drawer.boundingBox())!.height).toBeLessThanOrEqual(720);
  expect(await list.locator(".production-drawer__unit").count()).toBe(18);
  await page.getByTestId("produce-unit-btn-build|unit17|1").click();
  await expect(page.getByTestId("produce-count-build|unit17|1")).toHaveText("1");
  await page.getByRole("button", { name: "Reset selection" }).click();
  await expect(page.getByTestId("produce-count-build|unit17|1")).toHaveText("0");
  expect(submissions).toHaveLength(0);
});

test("resource payment accepts real pointer clicks, retains draft on minimize, and submits an offered ID", async ({
  page,
}) => {
  const payment: typeof initial = {
    ...initial,
    pending_choice: {
      nonce: "pay-7",
      choice: {
        player: seat,
        prompt: "Pay 4 resources",
        context: {
          subtype: "pay_resources",
          outstanding: [{ kind: "resources", amount: 4, paid: 0 }],
        },
        options: [
          {
            id: "exhaust|jord",
            kind: "pay",
            label: "Exhaust Jord",
            payload: { worth: 4, planet_name: "Jord" },
          },
          { id: "trade_good", kind: "pay", label: "Spend a trade good", payload: { worth: 1 } },
        ],
      },
    },
  };
  const batches = await mockBatchSubmission(page, payment);
  const { socket } = await openMockedGame(page, payment);
  const submissions: ClientMessage[] = [];
  socket.onMessage((data) => {
    const message = JSON.parse(String(data)) as ClientMessage;
    if (message.type === "submit_choice") submissions.push(message);
  });
  await expect(page.getByTestId("decision-modal")).toBeVisible();
  await page.getByTestId("planet-card-exhaust|jord").click();
  await expect(page.getByTestId("committed-amount")).toHaveText("4 Resources");
  await page.getByRole("button", { name: "Minimize decision" }).click();
  await expect(page.getByTestId("resume-decision-btn")).toBeVisible();
  await page.getByTestId("resume-decision-btn").click();
  await expect(page.getByTestId("committed-amount")).toHaveText("4 Resources");
  await page.getByTestId("confirm-payment-btn").click();
  await expect.poll(() => batches.length).toBe(1);
  expect(batches[0]).toMatchObject({
    expected_version: 7,
    nonce: "pay-7",
    plan: { kind: "payment", steps: [{ kind: "exhaust", planet: "jord" }] },
  });
  expect(batches[0].request_id).toBeTruthy();
  expect(submissions).toHaveLength(0);
});

test("payment controls remain reachable on a mobile viewport", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 780 });
  const payment: typeof initial = {
    ...initial,
    pending_choice: {
      nonce: "mobile-pay",
      choice: {
        player: seat,
        prompt: "Pay 1 resource",
        context: {
          subtype: "pay_resources",
          outstanding: [{ kind: "resources", amount: 1, paid: 0 }],
        },
        options: [
          { id: "trade_good", kind: "pay", label: "Spend a trade good", payload: { worth: 1 } },
        ],
      },
    },
  };
  await openMockedGame(page, payment);
  const dialog = page.getByTestId("decision-modal");
  await expect(dialog).toBeVisible();
  expect((await dialog.boundingBox())!.width).toBe(390);
  await page.getByTestId("tg-increment-btn").click();
  await expect(page.getByTestId("confirm-payment-btn")).toBeEnabled();
});

test("rejected movement submission stays actionable and retries with a fresh server nonce", async ({
  page,
}) => {
  const { socket, subscribe } = await openMockedGame(page);
  expect(subscribe).toEqual({
    type: "subscribe",
    protocol_version: PROTOCOL_VERSION,
    game_id: gameId,
    player_session: session,
  });
  const submissions: Extract<ClientMessage, { type: "submit_choice" }>[] = [];
  const sent = () => submissions.length;
  socket.onMessage((data) => {
    const message = JSON.parse(String(data)) as ClientMessage;
    if (message.type === "submit_choice") submissions.push(message);
  });

  const tray = page.getByTestId("tactical-movement-tray");
  const finish = page.getByTestId("commit-moves-btn");
  await expect(tray).toBeVisible();
  await expect(page.getByText("No ships eligible to move into the active system.")).toBeVisible();
  await expect(finish).toBeEnabled();
  await finish.focus();
  await expect(finish).toBeFocused();
  await page.keyboard.press("Enter");
  await expect.poll(sent, { timeout: 5_000 }).toBe(1);
  expect(submissions[0]).toEqual({
    type: "submit_choice",
    protocol_version: PROTOCOL_VERSION,
    game_id: gameId,
    option_id: "done_moving",
    nonce,
    expected_version: 7,
  });

  socket.send(
    JSON.stringify({
      type: "action_rejected",
      protocol_version: PROTOCOL_VERSION,
      game_id: gameId,
      game_version: 7,
      reason: { reason: "stale_nonce" },
    }),
  );
  await expect(tray.getByRole("alert")).toContainText("Stale decision nonce");
  await expect(finish).toBeEnabled();
  await expect(page.getByTestId("game-version")).toHaveText("v7");

  const fresh: StateUpdateMsg & { type: "state_update" } = {
    ...initial,
    type: "state_update",
    game_version: 8,
    pending_choice: { ...initial.pending_choice!, nonce: "movement-8" },
  };
  socket.send(JSON.stringify(fresh));
  socket.send(
    JSON.stringify({
      type: "pending_choice",
      protocol_version: PROTOCOL_VERSION,
      game_id: gameId,
      game_version: 8,
      nonce: "movement-8",
      choice: fresh.pending_choice!.choice,
      state: fresh.state,
      galaxy_layout: fresh.galaxy_layout,
    }),
  );
  await expect(page.getByTestId("game-version")).toHaveText("v8");
  await expect(finish).toBeEnabled();
  await finish.click(); // Real pointer hit target, after the refusal and authoritative refresh.
  await expect.poll(sent, { timeout: 5_000 }).toBe(2);
  expect(submissions[1]).toEqual({
    type: "submit_choice",
    protocol_version: PROTOCOL_VERSION,
    game_id: gameId,
    option_id: "done_moving",
    nonce: "movement-8",
    expected_version: 8,
  });

  socket.send(
    JSON.stringify({
      type: "action_accepted",
      protocol_version: PROTOCOL_VERSION,
      game_id: gameId,
      game_version: 8,
      option_id: "done_moving",
    }),
  );
  await expect(tray).toBeVisible(); // An acknowledgement alone cannot finish the workflow.
  await expect(finish).toBeDisabled();
  await expect(page.getByTestId("game-version")).toHaveText("v8");
  const next: StateUpdateMsg & { type: "state_update" } = {
    ...fresh,
    game_version: 9,
    pending_choice: null,
    turn_status: { kind: "active_turn", player: seat, phase: "action", round: 1 },
  };
  socket.send(JSON.stringify(next));
  await expect(page.getByTestId("game-version")).toHaveText("v9");
  await expect(tray).toHaveCount(0);
});
