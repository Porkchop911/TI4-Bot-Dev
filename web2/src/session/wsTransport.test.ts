import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { PlanStep } from "./movementPlan";
import type { TransportEvent } from "./transport";
import {
  type Catalog,
  type ClientMessage,
  type DraftOutcome,
  PROTOCOL_VERSION,
  type ReactionFacts,
} from "./wire";
import { GAME_UNKNOWN, type OnlineGame, type Socket, createWsTransport } from "./wsTransport";

// The messages that the server wrote (`crates/ti4-server/fixtures`, checked against the server
// by `tests/it/fixtures_verification.rs`), replayed over a socket of the test. The transport
// against a real server is in `transportContract.test.ts`.
const fixture = (name: string) =>
  JSON.parse(
    readFileSync(
      fileURLToPath(new URL(`../../../crates/ti4-server/fixtures/${name}.json`, import.meta.url)),
      "utf8",
    ),
  );
const actor = fixture("actor_snapshot");
const opponent = fixture("opponent_snapshot");
const spectator = fixture("spectator_snapshot");
const stale = fixture("stale_submission_rejected");
const gameOver = fixture("terminal_game_over");

const GAME: OnlineGame = { gameId: actor.game_id, playerSession: "secret" };
/** A page whose engine has the corpus of the server. */
const engine = { catalog: { content: actor.content } as Catalog };

/** The sockets that the transport opened, the newest last, and what it sent on each. */
function server(game: OnlineGame = GAME, catalog = engine, known = true) {
  const sockets: (Socket & { sent: ClientMessage[]; closed: boolean })[] = [];
  const transport = createWsTransport(catalog, game, {
    open(gameId) {
      expect(gameId).toBe(game.gameId);
      const socket = {
        sent: [] as ClientMessage[],
        closed: false,
        send: (data: string) => socket.sent.push(JSON.parse(data)),
        close: () => {
          socket.closed = true;
        },
        onopen: null,
        onmessage: null,
        onclose: null,
      } as (typeof sockets)[number];
      sockets.push(socket);
      return socket;
    },
    retryAfter: (attempt) => 100 * 2 ** attempt,
    known: async () => known,
  });
  const events: TransportEvent[] = [];
  transport.subscribe((event) => events.push(event));
  const socket = () => sockets.at(-1)!;
  return {
    transport,
    sockets,
    events,
    /** The events since the last call. */
    take: () => events.splice(0),
    sent: () => socket().sent,
    open: () => socket().onopen?.(),
    tell: (message: unknown) => socket().onmessage?.({ data: JSON.stringify(message) }),
    drop: (code?: number) => socket().onclose?.({ code }),
  };
}

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

it("subscribes as the player, and the snapshot is the first update", () => {
  const game = server();
  expect(game.take()).toEqual([{ kind: "modes", modes: {} }]);
  expect(game.sent()).toEqual([]);
  game.open();
  expect(game.sent()).toEqual([
    {
      type: "subscribe",
      protocol_version: PROTOCOL_VERSION,
      game_id: GAME.gameId,
      player_session: "secret",
      staged: true,
    },
  ]);
  game.tell(actor);
  const [event] = game.take();
  if (event.kind !== "update") {
    throw new Error(event.kind);
  }
  expect(event.canUndo).toBe(false);
  expect(event.update.viewer).toEqual({ role: "player", seat: "seat_a" });
  expect(event.update.pending_choice?.nonce).toBe("nonce_xyz789");
  expect(event.update.view).toEqual(actor.view);
  expect(event.update.score).toEqual(actor.score);
  // The redacted state of the message is not the page's to read.
  expect(event.update).not.toHaveProperty("state");
  expect(game.transport.capabilities).toEqual({});
  // A listener that comes later is told the same.
  const late: TransportEvent[] = [];
  game.transport.subscribe((told) => late.push(told));
  expect(late).toEqual([{ kind: "modes", modes: {} }, event]);
});

it("shows a seat that is not asked and a spectator the game with no choice", () => {
  const seat = server();
  seat.open();
  seat.tell(opponent);
  const [update] = seat.take().slice(-1);
  expect(update.kind === "update" && update.update.viewer).toEqual({
    role: "player",
    seat: "seat_b",
  });
  expect(update.kind === "update" && update.update.pending_choice).toBeUndefined();
  seat.transport.submitChoice("nonce_xyz789", "tactical");
  expect(seat.sent()).toHaveLength(1);

  const watcher = server({ gameId: actor.game_id, playerSession: null });
  watcher.open();
  expect(watcher.sent()[0]).not.toHaveProperty("player_session");
  // A spectator has nothing staged.
  expect(watcher.sent()[0]).not.toHaveProperty("staged");
  expect(watcher.transport.staged).toBeUndefined();
  watcher.tell(spectator);
  const [seen] = watcher.take().slice(-1);
  expect(seen.kind === "update" && seen.update.viewer).toEqual({ role: "spectator" });
  // A spectator has no turn to draft: the answer is null, and the server is not asked.
  const script = [{ kind: "choose" as const, option_id: "tactical" }];
  watcher.transport.draft(script);
  return vi.advanceTimersByTimeAsync(0).then(() => {
    expect(watcher.take()).toEqual([{ kind: "draft", script, outcome: null }]);
    expect(watcher.sent()).toHaveLength(1);
  });
});

it("answers with the version of the last update, and says when the answer was stale", () => {
  const game = server();
  game.open();
  game.tell(actor);
  const [first] = game.take().slice(-1);
  game.transport.submitChoice("another nonce", "tactical");
  expect(game.sent()).toHaveLength(1);
  game.transport.submitChoice("nonce_xyz789", "tactical");
  expect(game.sent()[1]).toEqual({
    type: "submit_choice",
    protocol_version: PROTOCOL_VERSION,
    game_id: GAME.gameId,
    nonce: "nonce_xyz789",
    expected_version: 42,
    option_id: "tactical",
  });
  // The rejection is never silent: a reason, and the game as it is once more.
  game.tell(stale);
  expect(game.take()).toEqual([
    { kind: "rejected", reason: "The game went on before your answer arrived. Nothing was sent." },
    first,
  ]);
  // No update came that explains it: the server is asked for the game again, on a new socket.
  vi.advanceTimersByTime(1_000);
  expect(game.sockets).toHaveLength(2);
  expect(game.sockets[0].closed).toBe(true);
  game.open();
  expect(game.sent()[0].type).toBe("subscribe");
  game.tell({ ...actor, game_version: 43 });
  expect(game.take().map((event) => event.kind)).toEqual(["update"]);
  game.transport.submitChoice("nonce_xyz789", "pass");
  expect(game.sent()[1]).toMatchObject({ expected_version: 43, option_id: "pass" });
});

it("does not ask again after a stale answer when the update came", () => {
  const game = server();
  game.open();
  game.tell(actor);
  game.transport.submitChoice("nonce_xyz789", "tactical");
  game.tell(stale);
  game.tell({ ...actor, type: "state_update", game_version: 43 });
  vi.advanceTimersByTime(5_000);
  expect(game.sockets).toHaveLength(1);
});

it("is quiet when another tab of the seat answered first, and the update shows the answer", () => {
  const game = server();
  game.open();
  game.tell(actor);
  game.take();
  game.transport.submitChoice("nonce_xyz789", "tactical");
  game.tell({
    type: "action_rejected",
    protocol_version: PROTOCOL_VERSION,
    reason: { reason: "answered_elsewhere" },
  });
  // No toast, and the server is not asked for the game again: its update is on its way.
  expect(game.take()).toEqual([]);
  vi.advanceTimersByTime(5_000);
  expect(game.sockets).toHaveLength(1);
  game.tell({ ...actor, type: "state_update", game_version: 43 });
  expect(game.take().map((event) => event.kind)).toEqual(["update"]);
});

it("reads the end of the game from the update, not from the message of it", () => {
  const game = server();
  game.open();
  game.tell(actor);
  game.take();
  game.tell(gameOver);
  expect(game.take()).toEqual([]);
});

it("refuses a server of another protocol or with other cards", () => {
  const other = server();
  other.open();
  other.tell({ ...actor, protocol_version: PROTOCOL_VERSION + 1 });
  expect(other.take().slice(-1)).toEqual([
    {
      kind: "error",
      message: `The server speaks protocol ${PROTOCOL_VERSION + 1} and this page protocol ${PROTOCOL_VERSION}. Reload the page.`,
    },
  ]);
  // Nothing is tried after it.
  expect(other.sockets[0].closed).toBe(true);
  vi.advanceTimersByTime(60_000);
  expect(other.sockets).toHaveLength(1);

  const cards = server(GAME, { catalog: { content: "0123456789abcdef" } as Catalog });
  cards.open();
  cards.tell(actor);
  const [event] = cards.take().slice(-1);
  expect(event.kind === "error" && event.message).toContain(
    `${actor.content.slice(0, 12)} there, 0123456789ab here`,
  );
  // A listener that comes later is told why there is no game.
  const late: TransportEvent[] = [];
  cards.transport.subscribe((told) => late.push(told));
  expect(late.at(-1)).toEqual(event);

  // An engine that does not say its corpus is not the one of the server either.
  const old = server(GAME, { catalog: {} as Catalog });
  old.open();
  old.tell(actor);
  expect(old.take().at(-1)?.kind).toBe("error");
});

it("says that the server does not know the player", () => {
  const game = server();
  game.open();
  game.tell({
    type: "error",
    protocol_version: PROTOCOL_VERSION,
    kind: "unauthorized",
    message: "Invalid player session",
  });
  expect(game.take().slice(-1)).toEqual([
    { kind: "error", message: "The server does not know the player of this page in this game." },
  ]);
});

const move: PlanStep = {
  kind: "move",
  origin: "01",
  unit: "carrier",
  damaged: false,
  gravity_drive: false,
  ionian: false,
};
const done: PlanStep = { kind: "done_moving" };

it("sends a plan as a script, and tells a refused plan with the same choice", () => {
  const game = server();
  game.open();
  game.tell(actor);
  const [first] = game.take().slice(-1);
  game.transport.submitPlan("nonce_xyz789", { destination: "23", steps: [move, done] });
  expect(game.sent()[1]).toEqual({
    type: "apply_draft",
    protocol_version: PROTOCOL_VERSION,
    game_id: GAME.gameId,
    nonce: "nonce_xyz789",
    expected_version: 42,
    script: [move, done],
  });
  // A result for another script is not of this plan.
  game.tell({
    type: "draft_applied",
    protocol_version: PROTOCOL_VERSION,
    game_id: GAME.gameId,
    nonce: "older",
    stopped: { applied: 0, remaining: [done], interrupted: false, reason: "No." },
  });
  expect(game.take()).toEqual([]);
  game.tell({
    type: "draft_applied",
    protocol_version: PROTOCOL_VERSION,
    game_id: GAME.gameId,
    nonce: "nonce_xyz789",
    stopped: { applied: 0, remaining: [move, done], interrupted: false, reason: "No carrier." },
  });
  expect(game.take()).toEqual([
    {
      ...first,
      plan: {
        destination: "23",
        applied: 0,
        remaining: [move, done],
        interrupted: false,
        reason: "No carrier.",
      },
    },
  ]);
  // The plan is told once.
  game.tell({ ...actor, type: "state_update", game_version: 43 });
  expect(game.take()).toEqual([{ ...first, plan: undefined }]);
});

it("tells where a draft that was sent stopped, in steps, with the update after it", () => {
  const game = server();
  game.open();
  game.tell(actor);
  game.take();
  game.transport.applyDraft("nonce_xyz789", {
    choices: ["tactical", "23"],
    plan: { destination: "23", steps: [move, done] },
  });
  expect(game.sent()[1]).toMatchObject({
    type: "apply_draft",
    script: [
      { kind: "choose", option_id: "tactical" },
      { kind: "choose", option_id: "23" },
      move,
      done,
    ],
  });
  // The action, the system and the ship are in the game; another decision came before the end.
  game.tell({
    type: "draft_applied",
    protocol_version: PROTOCOL_VERSION,
    game_id: GAME.gameId,
    nonce: "nonce_xyz789",
    stopped: { applied: 3, remaining: [done], interrupted: true, reason: "Another decision." },
  });
  expect(game.take()).toEqual([]);
  game.tell({ ...opponent, type: "state_update", game_version: 46 });
  const [after] = game.take();
  expect(after.kind === "update" && after.plan).toEqual({
    destination: "23",
    applied: 1,
    remaining: [done],
    interrupted: true,
    reason: "Another decision.",
  });

  // A script that stopped before its steps: the answers by id are not steps of the plan.
  game.tell({ ...actor, type: "state_update", game_version: 47 });
  game.take();
  game.transport.applyDraft("nonce_xyz789", {
    choices: ["tactical", "23"],
    plan: { destination: "23", steps: [move, done] },
  });
  game.tell({
    type: "draft_applied",
    protocol_version: PROTOCOL_VERSION,
    game_id: GAME.gameId,
    nonce: "nonce_xyz789",
    stopped: {
      applied: 1,
      remaining: [{ kind: "choose", option_id: "23" }, move, done],
      interrupted: true,
      reason: "Another decision.",
    },
  });
  game.tell({ ...actor, type: "state_update", game_version: 48 });
  const [early] = game.take();
  expect(early.kind === "update" && early.plan).toMatchObject({
    applied: 0,
    remaining: [move, done],
  });

  // A script that the game played whole has no result to tell.
  game.transport.submitPlan("nonce_xyz789", { destination: "23", steps: [done] });
  game.tell({
    type: "draft_applied",
    protocol_version: PROTOCOL_VERSION,
    game_id: GAME.gameId,
    nonce: "nonce_xyz789",
  });
  game.tell({ ...opponent, type: "state_update", game_version: 49 });
  const [whole] = game.take();
  expect(whole.kind === "update" && whole.plan).toBeUndefined();
});

it("asks for a draft with a request of its own, and drops the answer to an older one", () => {
  const game = server();
  game.open();
  game.tell(actor);
  game.take();
  const first = [{ kind: "choose" as const, option_id: "tactical" }];
  const second = [...first, { kind: "choose" as const, option_id: "23" }];
  game.transport.draft(first);
  game.transport.draft(second);
  const requests = game.sent().slice(1);
  expect(requests).toMatchObject([
    { type: "run_draft", script: first },
    { type: "run_draft", script: second },
  ]);
  const ids = requests.map((request) => (request.type === "run_draft" ? request.request_id : ""));
  expect(ids[0]).not.toBe(ids[1]);
  const outcome = { consumed: 2, stop: { kind: "open" } } as DraftOutcome;
  const answer = (request_id: string) =>
    game.tell({
      type: "draft_outcome",
      protocol_version: PROTOCOL_VERSION,
      game_id: GAME.gameId,
      request_id,
      base_version: 42,
      outcome,
    });
  answer(ids[0]);
  expect(game.take()).toEqual([]);
  answer(ids[1]);
  // The script of the request comes back, so that the page knows what the answer is of.
  const [event] = game.take();
  expect(event).toEqual({ kind: "draft", script: second, outcome });
  expect(event.kind === "draft" && event.script).toBe(second);
  // An answer is told once.
  answer(ids[1]);
  expect(game.take()).toEqual([]);
});

it("is offline while the server cannot be reached, and comes back with a snapshot", () => {
  const game = server();
  game.open();
  game.tell(actor);
  const [first] = game.take().slice(-1);
  game.drop();
  expect(game.take()).toEqual([{ kind: "offline" }]);
  // Nothing is sent while offline, and a listener that comes now is told so after the game.
  game.transport.submitChoice("nonce_xyz789", "tactical");
  game.transport.draft([]);
  const late: TransportEvent[] = [];
  game.transport.subscribe((told) => late.push(told));
  expect(late).toEqual([{ kind: "modes", modes: {} }, first, { kind: "offline" }]);
  // Each try waits longer than the one before, and offline is told once.
  expect(game.sockets).toHaveLength(1);
  vi.advanceTimersByTime(100);
  expect(game.sockets).toHaveLength(2);
  expect(game.sent()).toEqual([]);
  game.drop();
  vi.advanceTimersByTime(199);
  expect(game.sockets).toHaveLength(2);
  vi.advanceTimersByTime(1);
  expect(game.sockets).toHaveLength(3);
  expect(game.take()).toEqual([]);
  // The stored player session subscribes again, and the snapshot is the game as it is now.
  game.open();
  expect(game.sent()).toMatchObject([{ type: "subscribe", player_session: "secret" }]);
  game.tell(actor);
  expect(game.take()).toEqual([first]);
  // The count of the tries starts again.
  game.drop();
  vi.advanceTimersByTime(100);
  expect(game.sockets).toHaveLength(4);
});

it("says that the server no longer knows the game, and does not try again", async () => {
  const game = server(GAME, engine, false);
  game.take();
  // The server refuses the socket of a game that it does not have: it closes before it opens.
  game.drop();
  expect(game.take()).toEqual([{ kind: "offline" }]);
  await vi.advanceTimersByTimeAsync(0);
  expect(game.take()).toEqual([{ kind: "error", message: GAME_UNKNOWN }]);
  await vi.advanceTimersByTimeAsync(60_000);
  expect(game.sockets).toHaveLength(1);
  expect(game.take()).toEqual([]);
});

it("keeps trying a game that the server has while the socket does not open", async () => {
  const game = server();
  game.take();
  game.drop();
  await vi.advanceTimersByTimeAsync(100);
  expect(game.sockets).toHaveLength(2);
  expect(game.take()).toEqual([{ kind: "offline" }]);
});

it("comes back at once when the server replaced the game", () => {
  const game = server();
  game.open();
  game.tell(actor);
  game.drop(4001);
  vi.advanceTimersByTime(0);
  expect(game.sockets).toHaveLength(2);
});

it("takes back the last decision when the server says that the seat may", () => {
  const game = server();
  game.open();
  // The server says nothing of an undo: there is none, and nothing is sent.
  game.tell(actor);
  game.transport.undo();
  expect(game.sent()).toHaveLength(1);
  const may = { ...actor, game_version: actor.game_version + 1, history: { can_undo: true } };
  game.tell(may);
  const [open] = game.take().slice(-1);
  expect(open.kind === "update" && open.canUndo).toBe(true);
  // One undo at a time, with the version of the game that it is of.
  game.transport.undo();
  game.transport.undo();
  expect(game.sent().slice(1)).toEqual([
    {
      type: "undo",
      protocol_version: PROTOCOL_VERSION,
      game_id: GAME.gameId,
      expected_version: may.game_version,
    },
  ]);
  // The server plays the game again and closes the socket as replaced. That is not offline.
  game.drop(4001);
  vi.advanceTimersByTime(0);
  expect(game.sockets).toHaveLength(2);
  expect(game.take()).toEqual([]);
  game.open();
  // The snapshot of the game that the undo left has the steps that it took back, told once.
  const undone: PlanStep[] = [
    {
      kind: "move",
      origin: "18",
      unit: "cruiser",
      damaged: false,
      gravity_drive: false,
      ionian: false,
    },
    { kind: "done_moving" },
  ];
  game.tell({ ...actor, game_version: may.game_version + 2, undone });
  const [event] = game.take();
  if (event.kind !== "update") {
    throw new Error(event.kind);
  }
  expect(event.undone).toEqual(undone);
  expect(event.canUndo).toBe(false);
  const late: TransportEvent[] = [];
  game.transport.subscribe((told) => late.push(told));
  expect(late.at(-1)).not.toHaveProperty("undone");
});

it("says the barrier when the server does not take an undo back", () => {
  const game = server();
  game.open();
  game.tell({ ...actor, history: { can_undo: true } });
  game.take();
  game.transport.undo();
  game.tell({
    type: "action_rejected",
    protocol_version: PROTOCOL_VERSION,
    reason: { reason: "undo_barrier", barrier: "random" },
  });
  const [rejected, again] = game.take();
  expect(rejected).toEqual({
    kind: "rejected",
    reason: "Dice were rolled since your last decision. It cannot be taken back.",
  });
  expect(again.kind).toBe("update");
  // The undo is over: a socket that ends now is a lost connection.
  game.drop(4001);
  expect(game.take()).toEqual([{ kind: "offline" }]);
});

it("pings while it is connected, and stops when it is closed", () => {
  const game = server();
  game.open();
  game.tell(actor);
  vi.advanceTimersByTime(20_000);
  expect(game.sent().slice(1)).toEqual([
    { type: "ping", protocol_version: PROTOCOL_VERSION, sequence: 1 },
    { type: "ping", protocol_version: PROTOCOL_VERSION, sequence: 2 },
  ]);
  game.transport.close();
  expect(game.sockets[0].closed).toBe(true);
  vi.advanceTimersByTime(60_000);
  expect(game.sent()).toHaveLength(3);
  expect(game.sockets).toHaveLength(1);
});

it("tells the Never cards as the server keeps them, and a window that was passed", () => {
  const game = server();
  game.open();
  game.tell({ ...actor, reaction_modes: { Sabotage: "never" } });
  const [cards, first] = game.take().slice(-2);
  expect(cards).toEqual({ kind: "modes", modes: { Sabotage: "never" } });
  // The page asks; the setting is the server's.
  game.transport.setReactionMode("Sabotage", "never");
  expect(game.sent()).toHaveLength(1);
  game.transport.setReactionMode("Scuttle", "never");
  expect(game.sent()[1]).toEqual({
    type: "set_reaction_mode",
    protocol_version: PROTOCOL_VERSION,
    game_id: GAME.gameId,
    card: "Scuttle",
    mode: "never",
  });
  // The server tells the game again with the setting. The game is the same: no update, so
  // what the page has staged stays.
  const update = { ...actor, type: "state_update" };
  game.tell({ ...update, reaction_modes: { Sabotage: "never", Scuttle: "never" } });
  expect(game.take()).toEqual([{ kind: "modes", modes: { Sabotage: "never", Scuttle: "never" } }]);
  game.tell({ ...update, reaction_modes: { Scuttle: "never" } });
  expect(game.take()).toEqual([{ kind: "modes", modes: { Scuttle: "never" } }]);
  // "Only on my turns" is a mode of its own, and the card of an ability is set by its key.
  game.tell({ ...update, reaction_modes: { Scuttle: "never", "leader:solagent": "ownturn" } });
  expect(game.take()).toEqual([
    { kind: "modes", modes: { Scuttle: "never", "leader:solagent": "ownturn" } },
  ]);
  game.tell({ ...update, reaction_modes: { Scuttle: "never" } });
  expect(game.take()).toEqual([{ kind: "modes", modes: { Scuttle: "never" } }]);
  // A window of a Never card was declined on the server: told once, before the update.
  const passed = {
    ...update,
    game_version: 43,
    reaction_modes: { Scuttle: "never" },
    auto_resolved: [
      { id: "auto-1", prompt: "p", selected: "Pass", reason: "r", passed: ["Scuttle"] },
      { id: "auto-2", prompt: "pay 1 more", selected: "trade goods", reason: "only way" },
    ],
  };
  game.tell(passed);
  expect(game.take()).toEqual([
    { kind: "passed", cards: ["Scuttle"] },
    { ...first, plan: undefined },
  ]);
  game.tell({ ...passed, game_version: 44 });
  expect(game.take().map((event) => event.kind)).toEqual(["update"]);
});

it("hands on the reaction window that the viewer is asked in", () => {
  const game = server();
  game.open();
  const reaction: ReactionFacts = {
    own_turn_applies: true,
    offers: [{ option: "opt_1", key: "Sabotage", kind: "action_card", name: "Sabotage" }],
  };
  game.tell({ ...actor, reaction });
  const event = game.take().at(-1);
  expect(event).toMatchObject({ kind: "update", update: { reaction } });
});

it("tells who the game waits for while a seat of the server takes its time", () => {
  const game = server();
  game.open();
  game.tell(opponent);
  game.take();
  const status = {
    kind: "waiting_for_decision",
    seat: "seat_b",
    phase: "action",
    round: 1,
    stage: "Action Phase",
  };
  const message = {
    type: "turn_status",
    protocol_version: PROTOCOL_VERSION,
    game_id: GAME.gameId,
    game_version: opponent.game_version,
    status,
  };
  game.tell(message);
  // Nothing else of the game is new: it is no update, so what is staged stays.
  expect(game.take()).toEqual([{ kind: "status", status }]);
  // A listener that comes later is told the game with that status.
  const late: TransportEvent[] = [];
  game.transport.subscribe((told) => late.push(told));
  expect(late.at(-1)).toMatchObject({ kind: "update", update: { turn_status: status } });
  // The status of another version belongs to an update that is on its way.
  game.tell({ ...message, game_version: opponent.game_version + 1 });
  expect(game.take()).toEqual([]);
});

/** A `seat_staged` of the server: the slots of the seat, by revision. */
const seatStaged = (
  revision: number,
  slots: Record<string, { revision: number; value?: unknown }>,
  more: { writer?: number; connection?: number } = {},
) => ({
  type: "seat_staged",
  protocol_version: PROTOCOL_VERSION,
  game_id: GAME.gameId,
  revision,
  slots,
  ...more,
});

/** The writes of the staged work that the transport sent. */
const puts = (game: ReturnType<typeof server>) =>
  game.sent().filter((message) => message.type === "put_staged");

it("keeps what the seat staged on the server: one write on its way, and the echo is its base", () => {
  const game = server();
  game.open();
  game.tell(actor);
  const store = game.transport.staged!;
  const told: unknown[] = [];
  store.subscribe((slot, value) => told.push([slot, value]));
  // Before the server said who this connection is, nothing is written.
  store.set("plan", { cursor: 0 });
  vi.advanceTimersByTime(1_000);
  expect(puts(game)).toEqual([]);
  game.tell(seatStaged(0, {}, { connection: 7 }));
  expect(puts(game)).toEqual([
    {
      type: "put_staged",
      protocol_version: PROTOCOL_VERSION,
      game_id: GAME.gameId,
      slot: "plan",
      base_revision: 0,
      value: { cursor: 0 },
    },
  ]);
  // A change waits a moment for the next one: a movement is many clicks.
  store.set("plan", { cursor: 1 });
  store.set("plan", { cursor: 2 });
  expect(store.get("plan")).toEqual({ cursor: 2 });
  vi.advanceTimersByTime(250);
  // One write of a slot is on its way at a time: the next waits for the echo of the first.
  expect(puts(game)).toHaveLength(1);
  game.tell(seatStaged(1, { plan: { revision: 1, value: { cursor: 0 } } }, { writer: 7 }));
  expect(puts(game)).toHaveLength(2);
  expect(puts(game)[1]).toMatchObject({ slot: "plan", base_revision: 1, value: { cursor: 2 } });
  // Its own writes are not told as a change of another tab.
  game.tell(seatStaged(2, { plan: { revision: 2, value: { cursor: 2 } } }, { writer: 7 }));
  expect(told).toEqual([]);
  // A value that the server has is not written again: the server would not answer.
  store.set("plan", { cursor: 2 });
  vi.advanceTimersByTime(250);
  expect(puts(game)).toHaveLength(2);
  // The order of the keys is no change: the server keeps them sorted.
  store.set("movement", { b: 1, a: 2 });
  vi.advanceTimersByTime(250);
  game.tell(
    seatStaged(
      3,
      {
        plan: { revision: 2, value: { cursor: 2 } },
        movement: { revision: 3, value: { a: 2, b: 1 } },
      },
      { writer: 7 },
    ),
  );
  store.set("movement", { b: 1, a: 2 });
  vi.advanceTimersByTime(250);
  expect(puts(game)).toHaveLength(3);
  // A slot is cleared with no value.
  store.set("movement", null);
  vi.advanceTimersByTime(250);
  expect(puts(game)[3]).toEqual({
    type: "put_staged",
    protocol_version: PROTOCOL_VERSION,
    game_id: GAME.gameId,
    slot: "movement",
    base_revision: 3,
  });
});

it("takes what another tab of the seat staged, also over a write of its own that was stale", () => {
  const game = server();
  game.open();
  game.tell(actor);
  const store = game.transport.staged!;
  const told: unknown[] = [];
  store.subscribe((slot, value) => told.push([slot, value]));
  // The server has what the seat staged before: it is the page's, and told.
  game.tell(seatStaged(4, { secondary: { revision: 4, value: { card: "x" } } }, { connection: 2 }));
  expect(store.get("secondary")).toEqual({ card: "x" });
  expect(told).toEqual([["secondary", { card: "x" }]]);
  // This tab writes on revision 4, and another tab was there first.
  store.set("secondary", { card: "mine" });
  vi.advanceTimersByTime(250);
  expect(puts(game)).toEqual([expect.objectContaining({ base_revision: 4 })]);
  game.tell(seatStaged(5, { secondary: { revision: 5, value: { card: "other" } } }, { writer: 3 }));
  expect(store.get("secondary")).toEqual({ card: "other" });
  expect(told.at(-1)).toEqual(["secondary", { card: "other" }]);
  // The answer to the stale write is the slot as it is: nothing new, and nothing written again.
  game.tell(seatStaged(5, { secondary: { revision: 5, value: { card: "other" } } }));
  vi.advanceTimersByTime(1_000);
  expect(puts(game)).toHaveLength(1);
  expect(told).toHaveLength(2);
  // A newer value of the server replaces one that was not sent yet.
  store.set("secondary", { card: "later" });
  game.tell(seatStaged(6, { secondary: { revision: 6 } }, { writer: 3 }));
  vi.advanceTimersByTime(1_000);
  expect(store.get("secondary")).toBeNull();
  expect(puts(game)).toHaveLength(1);
});

it("says a write that the server did not take in words of the page, and does not try it again", () => {
  const game = server();
  game.open();
  game.tell(actor);
  game.take();
  const store = game.transport.staged!;
  game.tell(seatStaged(0, {}, { connection: 1 }));
  store.set("plan", { cursor: 0 });
  vi.advanceTimersByTime(250);
  game.tell({
    type: "error",
    protocol_version: PROTOCOL_VERSION,
    kind: "malformed_message",
    message: "too many staged writes; wait a moment",
  });
  game.tell(seatStaged(0, {}));
  expect(game.take()).toEqual([
    { kind: "rejected", reason: "The server did not keep what you staged. It stays in this tab." },
  ]);
  vi.advanceTimersByTime(10_000);
  expect(puts(game)).toHaveLength(1);
  expect(store.get("plan")).toEqual({ cursor: 0 });
  // The next change is written.
  store.set("plan", { cursor: 1 });
  vi.advanceTimersByTime(250);
  expect(puts(game)).toHaveLength(2);
});

it("writes again after a lost connection what the server did not get", () => {
  const game = server();
  game.open();
  game.tell(actor);
  const store = game.transport.staged!;
  game.tell(seatStaged(0, {}, { connection: 1 }));
  store.set("plan", { cursor: 0 });
  vi.advanceTimersByTime(250);
  expect(puts(game)).toHaveLength(1);
  game.drop();
  vi.advanceTimersByTime(100);
  game.open();
  game.tell(actor);
  expect(puts(game)).toEqual([]);
  // The new connection has another id; the server did not write the value.
  game.tell(seatStaged(0, {}, { connection: 2 }));
  expect(puts(game)).toEqual([expect.objectContaining({ base_revision: 0, value: { cursor: 0 } })]);
  game.tell(seatStaged(1, { plan: { revision: 1, value: { cursor: 0 } } }, { writer: 2 }));
  store.set("plan", { cursor: 1 });
  vi.advanceTimersByTime(250);
  expect(puts(game)[1]).toMatchObject({ base_revision: 1 });
});

it("puts once what a page of before kept in this browser, when the server has nothing", () => {
  const dropped: string[] = [];
  const legacy = {
    read: () => ({ plan: { cursor: 0 }, movement: { nonce: "n" } }),
    drop: () => void dropped.push("all"),
  };
  const game = server({ ...GAME, legacyStaged: legacy });
  game.open();
  game.tell(actor);
  const store = game.transport.staged!;
  const told: unknown[] = [];
  store.subscribe((slot) => told.push(slot));
  game.tell(seatStaged(0, {}, { connection: 1 }));
  expect(told).toEqual(["movement", "plan"]);
  expect(store.get("plan")).toEqual({ cursor: 0 });
  expect(puts(game).map((put) => put.type === "put_staged" && put.slot)).toEqual([
    "movement",
    "plan",
  ]);
  expect(dropped).toEqual(["all"]);
  // The server has what the seat staged: what the browser kept is dropped, not put.
  const later = server({ ...GAME, legacyStaged: legacy });
  later.open();
  later.tell(actor);
  later.tell(seatStaged(2, { plan: { revision: 2, value: { cursor: 5 } } }, { connection: 3 }));
  expect(later.transport.staged!.get("plan")).toEqual({ cursor: 5 });
  expect(puts(later)).toEqual([]);
  expect(dropped).toEqual(["all", "all"]);
});
