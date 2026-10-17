import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Socket } from "@ti4/shared/net/socket.ts";
import {
  type Seat,
  TABLE_UNKNOWN,
  type TableEvent,
  createTable,
  playerOf,
  serverTable,
} from "./table";
import { type LobbyState, PROTOCOL_VERSION } from "./wire";

const SEAT: Seat = { playerId: "ada", playerSession: "secret" };

const lobby = (version: number, change: Partial<LobbyState> = {}): LobbyState => ({
  game_id: "g1",
  phase: "lobby",
  host_player_id: "ada",
  slots: [
    {
      slot_id: "slot_1",
      position: 1,
      occupant: "ada",
      nickname: "Ada",
      connected: true,
      can_take_over: false,
    },
    {
      slot_id: "slot_2",
      position: 2,
      occupant: null,
      nickname: null,
      connected: false,
      can_take_over: false,
    },
  ],
  lobby_version: version,
  bot_kinds: ["random", "smart"],
  map: { kind: "random", systems: 19, hyperlanes: false, recommended: false },
  map_revision: 0,
  strategy_card_set: "te",
  speaker_order: "random",
  factions: [],
  bot_delay_ms: 600,
  ...change,
});

interface Call {
  url: string;
  method: string;
  session: string | null;
  body: unknown;
}
type Answer = { status: number; body: unknown } | "unreachable";

/** A table over a socket of the test and a `fetch` of the test. */
function server(seat: Seat | null = SEAT, answers: Record<string, Answer> = {}) {
  const sockets: (Socket & { sent: Record<string, unknown>[]; closed: boolean })[] = [];
  const calls: Call[] = [];
  const fetcher = (async (url: string, init?: RequestInit) => {
    const headers = (init?.headers ?? {}) as Record<string, string>;
    calls.push({
      url,
      method: init?.method ?? "GET",
      session: headers["x-ti4-player-session"] ?? null,
      body: init?.body ? JSON.parse(String(init.body)) : null,
    });
    const answer = answers[url.replace("/api/games/g1/lobby", "") || "/"] ?? {
      status: 404,
      body: "Game not found",
    };
    if (answer === "unreachable") {
      throw new TypeError("fetch failed");
    }
    const text = typeof answer.body === "string" ? answer.body : JSON.stringify(answer.body);
    return {
      ok: answer.status < 300,
      status: answer.status,
      text: async () => text,
      json: async () => JSON.parse(text),
    };
  }) as unknown as typeof fetch;
  const table = serverTable("g1", seat, {
    open(gameId) {
      expect(gameId).toBe("g1");
      const socket = {
        sent: [] as Record<string, unknown>[],
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
    fetch: fetcher,
  });
  const events: TableEvent[] = [];
  table.subscribe((event) => events.push(event));
  const socket = () => sockets.at(-1)!;
  return {
    table,
    sockets,
    calls,
    answers,
    /** The events since the last call. */
    take: () => events.splice(0),
    sent: () => socket().sent,
    open: () => socket().onopen?.(),
    tell: (message: Record<string, unknown>) =>
      socket().onmessage?.({
        data: JSON.stringify({ protocol_version: PROTOCOL_VERSION, ...message }),
      }),
    push: (view: LobbyState, viewer: string | null = seat?.playerId ?? null) =>
      socket().onmessage?.({
        data: JSON.stringify({
          type: "table",
          protocol_version: PROTOCOL_VERSION,
          ...(viewer === null ? {} : { viewer }),
          lobby: view,
        }),
      }),
    drop: () => socket().onclose?.({}),
  };
}

/** Lets the answers of the calls arrive. */
const settle = () => vi.advanceTimersByTimeAsync(0);
const versions = (events: TableEvent[]) =>
  events.map((event) => (event.kind === "table" ? event.lobby.lobby_version : event.kind));

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

it("subscribes as the player, and the table is told with its viewer", () => {
  const game = server();
  expect(game.sent()).toEqual([]);
  game.open();
  expect(game.sent()).toEqual([
    { type: "subscribe", protocol_version: PROTOCOL_VERSION, player_session: "secret" },
  ]);
  game.push(lobby(1));
  expect(game.take()).toEqual([
    { kind: "table", lobby: lobby(1), viewer: "ada", preview: null, options: null },
  ]);
  // A listener that comes later is told the table at once.
  const late: TableEvent[] = [];
  game.table.subscribe((event) => late.push(event));
  expect(versions(late)).toEqual([1]);
});

it("a visitor subscribes with no player session, and its calls have none", async () => {
  const game = server(null);
  game.open();
  expect(game.sent()).toEqual([{ type: "subscribe", protocol_version: PROTOCOL_VERSION }]);
  game.push(lobby(1));
  expect(game.take()).toMatchObject([{ kind: "table", viewer: null }]);
  await settle();
  expect(game.calls.every((call) => call.session === null)).toBe(true);
});

it("of two views the one with the higher version is kept", async () => {
  const game = server(SEAT, { "/bots": { status: 200, body: lobby(3) } });
  game.open();
  game.push(lobby(4));
  await settle();
  game.take();
  // The answer of a call is older than what the socket told already: it is dropped.
  game.table.addBots("smart");
  await settle();
  expect(game.calls.at(-1)).toMatchObject({
    url: "/api/games/g1/lobby/bots",
    method: "POST",
    session: "secret",
    body: { kind: "smart" },
  });
  expect(game.take()).toEqual([]);
  // So is an older view of the socket.
  game.push(lobby(2));
  expect(game.take()).toEqual([]);
  // A newer answer is the table at once, before the socket tells it; the socket's is then old news.
  game.answers["/bots"] = { status: 200, body: lobby(5) };
  game.table.addBots("smart", true);
  await settle();
  expect(game.calls.at(-1)?.body).toEqual({ kind: "smart", fill: true });
  expect(versions(game.take())).toEqual([5]);
  expect(game.take()).toEqual([]);
});

it("the socket tells who is connected with the same version, and an answer does not go back", async () => {
  const game = server(SEAT, { "/remove-bot": { status: 200, body: lobby(2) } });
  game.open();
  game.push(lobby(2));
  await settle();
  game.take();
  const away = lobby(2);
  away.slots[0].connected = false;
  game.push(away);
  expect(game.take()).toMatchObject([
    { kind: "table", lobby: { slots: [{ connected: false }, {}] } },
  ]);
  game.table.removeSeat("bot1");
  await settle();
  expect(game.calls.at(-1)?.body).toEqual({ player_id: "bot1" });
  expect(game.take()).toEqual([]);
});

it("a failed call is told with its reason, and the table stays as it was", async () => {
  const game = server(SEAT, {
    "/start": { status: 409, body: "Every seat must be filled before starting" },
    "/leave": "unreachable",
    "/bots": { status: 500, body: "" },
  });
  game.open();
  game.push(lobby(1));
  await settle();
  game.take();
  game.table.start();
  await settle();
  expect(game.take()).toEqual([
    { kind: "rejected", reason: "Every seat must be filled before starting" },
  ]);
  game.table.leave();
  await settle();
  expect(game.take()).toEqual([
    { kind: "rejected", reason: "The server cannot be reached. Nothing was changed." },
  ]);
  game.table.addBots("random");
  await settle();
  expect(game.take()).toEqual([{ kind: "rejected", reason: "The server refused (500)." }]);
  const now: TableEvent[] = [];
  game.table.subscribe((event) => now.push(event));
  expect(versions(now)).toEqual([1]);
});

it("a lost socket is offline once, and the page tries again by itself", async () => {
  const game = server(SEAT, { "/": { status: 200, body: lobby(1) } });
  game.open();
  game.push(lobby(1));
  await settle();
  game.take();
  game.drop();
  expect(game.take()).toEqual([{ kind: "offline" }]);
  expect(game.sockets).toHaveLength(1);
  await vi.advanceTimersByTimeAsync(100);
  expect(game.sockets).toHaveLength(2);
  // The server is still away: the next try waits twice as long, and nothing more is said.
  game.drop();
  await vi.advanceTimersByTimeAsync(199);
  expect(game.sockets).toHaveLength(2);
  await vi.advanceTimersByTimeAsync(1);
  expect(game.sockets).toHaveLength(3);
  expect(game.take()).toEqual([]);
  // A listener that comes now is told both: the last table, and that it is out of reach.
  const late: TableEvent[] = [];
  game.table.subscribe((event) => late.push(event));
  expect(versions(late)).toEqual([1, "offline"]);
  // The server is back: the page subscribes again, and the table ends the offline state.
  game.open();
  expect(game.sent()).toMatchObject([{ type: "subscribe", player_session: "secret" }]);
  game.push(lobby(2));
  expect(versions(game.take())).toEqual([2]);
  game.drop();
  expect(game.take()).toEqual([{ kind: "offline" }]);
  await vi.advanceTimersByTimeAsync(100);
  expect(game.sockets).toHaveLength(4);
});

it("an answer that comes while the socket is away does not end the offline state", async () => {
  const preview = { player_count: 2, tiles: [], seats: [] };
  const game = server(SEAT, {
    "/map-preview": { status: 200, body: preview },
    "/bots": { status: 200, body: lobby(2) },
  });
  game.open();
  game.push(lobby(1));
  // The socket is lost before the board is there.
  game.drop();
  await settle();
  expect(versions(game.take())).toEqual([1, "offline", 1, "offline"]);
  game.table.addBots("random");
  await settle();
  expect(versions(game.take())).toEqual([2, "offline"]);
  // Only a table of the socket ends it.
  await vi.advanceTimersByTimeAsync(100);
  game.open();
  game.push(lobby(2));
  expect(versions(game.take())).toEqual([2]);
});

it("a table that the server does not have is said, and nothing is tried after it", async () => {
  const game = server();
  // The server refuses the socket of a table that it does not have: it never opens.
  game.drop();
  await settle();
  expect(game.take()).toEqual([{ kind: "offline" }, { kind: "error", message: TABLE_UNKNOWN }]);
  await vi.advanceTimersByTimeAsync(60_000);
  expect(game.sockets).toHaveLength(1);

  // A server that cannot be reached is not such a table.
  const away = server(SEAT, { "/": "unreachable" });
  away.drop();
  await settle();
  expect(away.take()).toEqual([{ kind: "offline" }]);
  await vi.advanceTimersByTimeAsync(100);
  expect(away.sockets).toHaveLength(2);
});

it("the socket pings while it is open, so a silent lobby stays connected", async () => {
  const game = server();
  game.open();
  game.push(lobby(1));
  await vi.advanceTimersByTimeAsync(10_000);
  expect(game.sent().at(-1)).toEqual({ type: "ping", protocol_version: PROTOCOL_VERSION });
  game.tell({ type: "pong" });
  expect(game.take().filter((event) => event.kind !== "table")).toEqual([]);
});

it("the board is asked when the table is told, and again when its map changes", async () => {
  const preview = { player_count: 2, tiles: [], seats: [] };
  const game = server(SEAT, { "/map-preview": { status: 200, body: preview } });
  game.open();
  game.push(lobby(1));
  await settle();
  expect(game.take()).toMatchObject([
    { kind: "table", preview: null },
    { kind: "table", preview },
  ]);
  const asked = () => game.calls.filter((call) => call.url.endsWith("/map-preview")).length;
  expect(asked()).toBe(1);
  // A change that is not of the map asks nothing.
  game.push(lobby(2));
  await settle();
  expect(asked()).toBe(1);
  game.push(lobby(3, { map_revision: 1 }));
  await settle();
  expect(asked()).toBe(2);
});

it("the setup options are asked with the socket, until the server said them", async () => {
  const options = { min_players: 2, max_players: 8, scenarios: [] };
  const game = server(SEAT, { "/api/setup-options": "unreachable" });
  const asked = () => game.calls.filter((call) => call.url === "/api/setup-options").length;
  game.open();
  game.push(lobby(1));
  await settle();
  expect(asked()).toBe(1);
  expect(game.take()).toMatchObject([{ kind: "table", options: null }]);
  // A call that failed is made again with the next socket.
  game.answers["/api/setup-options"] = { status: 200, body: options };
  game.drop();
  await vi.advanceTimersByTimeAsync(100);
  game.open();
  game.push(lobby(1));
  await settle();
  expect(asked()).toBe(2);
  expect(game.take().at(-1)).toMatchObject({ kind: "table", options });
  game.drop();
  await vi.advanceTimersByTimeAsync(100);
  game.open();
  await settle();
  expect(asked()).toBe(2);
});

it("a change of the setup and a new name are calls of the lobby", async () => {
  const game = server(SEAT, {
    "/settings": { status: 200, body: lobby(2, { bot_delay_ms: 1000 }) },
    "/nickname": { status: 409, body: "The name is taken" },
  });
  game.open();
  game.push(lobby(1));
  await settle();
  game.take();
  game.table.setSettings({ waitMs: 1000 });
  await settle();
  expect(game.calls.at(-1)).toMatchObject({
    url: "/api/games/g1/lobby/settings",
    session: "secret",
    body: { bot_delay_ms: 1000 },
  });
  expect(game.take()).toMatchObject([{ kind: "table", lobby: { bot_delay_ms: 1000 } }]);
  // Only what changes is sent, and "round 1" is said as null.
  game.table.setSettings({ players: 5, cards: "pok", startFrom: null });
  await settle();
  expect(game.calls.at(-1)?.body).toEqual({
    player_count: 5,
    strategy_card_set: "pok",
    start_from: null,
  });
  game.take();
  game.table.rename("Ada Lovelace");
  await settle();
  expect(game.calls.at(-1)).toMatchObject({
    url: "/api/games/g1/lobby/nickname",
    body: { nickname: "Ada Lovelace" },
  });
  expect(game.take()).toEqual([{ kind: "rejected", reason: "The name is taken" }]);
});

it("a join is a new player for another tab", async () => {
  const joined = lobby(2);
  const game = server(SEAT, {
    "/join": {
      status: 200,
      body: { player_session: "other", player: { id: "ada2" }, lobby: joined },
    },
  });
  game.open();
  game.push(lobby(1));
  await settle();
  game.take();
  await expect(game.table.join("Ada 2")).resolves.toEqual({
    playerId: "ada2",
    playerSession: "other",
  });
  // With no player session: the server would answer with the player of this page.
  expect(game.calls.at(-1)).toMatchObject({
    session: null,
    body: { kind: "new", nickname: "Ada 2" },
  });
  // This page stays its player.
  expect(game.take()).toMatchObject([
    { kind: "table", viewer: "ada", lobby: { lobby_version: 2 } },
  ]);

  game.answers["/join"] = { status: 409, body: "Seat is unavailable" };
  await expect(game.table.join("Ada 3")).resolves.toBeNull();
  expect(game.take()).toEqual([{ kind: "rejected", reason: "Seat is unavailable" }]);
});

it("a player that the table no longer knows is shown the table as a visitor", () => {
  const game = server();
  game.open();
  game.tell({
    type: "error",
    kind: "unauthorized",
    message: "The table does not know this player session",
  });
  expect(game.sent().at(-1)).toEqual({ type: "subscribe", protocol_version: PROTOCOL_VERSION });
  game.push(lobby(1), null);
  expect(game.take()).toMatchObject([{ kind: "table", viewer: null }]);
});

it("a server of another protocol is refused with both versions", () => {
  const game = server();
  game.open();
  game.sockets[0].onmessage?.({
    data: JSON.stringify({
      type: "table",
      protocol_version: PROTOCOL_VERSION + 1,
      lobby: lobby(1),
    }),
  });
  expect(game.take()).toEqual([
    {
      kind: "error",
      message: `The server speaks protocol ${PROTOCOL_VERSION + 1} and this page protocol ${PROTOCOL_VERSION}. Reload the page.`,
    },
  ]);
  expect(game.sockets[0].closed).toBe(true);
});

it("a table is made with what the page asks, and the answer is its host", async () => {
  const calls: Call[] = [];
  const fetcher = (async (url: string, init?: RequestInit) => {
    calls.push({
      url,
      method: init?.method ?? "GET",
      session: null,
      body: JSON.parse(String(init?.body)),
    });
    return {
      ok: true,
      status: 200,
      json: async () => ({
        game_id: "g9",
        player_session: "s",
        player: { id: "p" },
        lobby: lobby(1),
      }),
    };
  }) as unknown as typeof fetch;
  await expect(
    createTable({ playerCount: 6, nickname: "Ada", seed: 3, map: "milty6" }, fetcher),
  ).resolves.toEqual({ gameId: "g9", playerId: "p", playerSession: "s" });
  await createTable(
    { playerCount: 8, nickname: "Ada", map: "random", fill: "random", waitMs: 1000 },
    fetcher,
  );
  expect(calls.map((call) => call.body)).toEqual([
    { player_count: 6, nickname: "Ada", seed: 3, map_template: "milty6" },
    { player_count: 8, nickname: "Ada", fill_seats: "random", bot_delay_ms: 1000, map: "random" },
  ]);
});

it("the player of a kept session is asked from the server", async () => {
  const answer = (ok: boolean) =>
    (async () => ({
      ok,
      status: ok ? 200 : 403,
      json: async () => ({ player: { id: "ada" } }),
    })) as unknown as typeof fetch;
  await expect(playerOf("g1", "secret", answer(true))).resolves.toBe("ada");
  await expect(playerOf("g1", "secret", answer(false))).resolves.toBeNull();
});

it("the faction of a seat is a call of the lobby, and a faction of another seat is refused", async () => {
  const game = server(SEAT, {
    "/seat-faction": { status: 409, body: "Hacan is already taken by Bo" },
  });
  game.open();
  game.push(lobby(1));
  await settle();
  game.take();
  game.table.setFaction("p1", "hacan");
  await settle();
  expect(game.calls.at(-1)).toMatchObject({
    url: "/api/games/g1/lobby/seat-faction",
    session: "secret",
    body: { player_id: "p1", faction: "hacan" },
  });
  expect(game.take()).toEqual([{ kind: "rejected", reason: "Hacan is already taken by Bo" }]);
  // Back to the draw of the start.
  game.table.setFaction("bot1", null);
  await settle();
  expect(game.calls.at(-1)?.body).toEqual({ player_id: "bot1", faction: null });
});

it("a visitor takes the seat of a person who is away, with no player session", async () => {
  const running = lobby(3, { phase: "running" });
  const game = server(null, {
    "/join": {
      status: 200,
      body: { player_session: "fresh", player: { id: "bo" }, lobby: running },
    },
  });
  game.open();
  game.push(lobby(2, { phase: "running" }));
  await settle();
  game.take();
  await expect(game.table.takeOver("bo", "Bo")).resolves.toEqual({
    playerId: "bo",
    playerSession: "fresh",
  });
  expect(game.calls.at(-1)).toMatchObject({
    url: "/api/games/g1/lobby/join",
    session: null,
    body: { kind: "takeover", player_id: "bo", nickname: "Bo" },
  });
  game.answers["/join"] = { status: 409, body: "Takeover is unavailable" };
  game.take();
  await expect(game.table.takeOver("bo", "Bo")).resolves.toBeNull();
  expect(game.take()).toEqual([{ kind: "rejected", reason: "Takeover is unavailable" }]);
});

it("the wait of a running game is a call of the game, and nothing else of its setup is sent", async () => {
  const game = server(SEAT, {
    "/api/games/g1/settings": {
      status: 200,
      body: lobby(3, { phase: "running", bot_delay_ms: 1500 }),
    },
  });
  game.open();
  game.push(lobby(2, { phase: "running" }));
  await settle();
  game.take();
  game.table.setSettings({ waitMs: 1500, players: 5 });
  await settle();
  expect(game.calls.at(-1)).toMatchObject({
    url: "/api/games/g1/settings",
    method: "POST",
    session: "secret",
    body: { bot_delay_ms: 1500 },
  });
  expect(game.take()).toMatchObject([
    { kind: "table", lobby: { phase: "running", bot_delay_ms: 1500 } },
  ]);
});

it("a running table has its board too", async () => {
  const preview = { player_count: 2, tiles: [], seats: [] };
  const game = server(null, { "/map-preview": { status: 200, body: preview } });
  game.open();
  game.push(lobby(2, { phase: "running" }));
  await settle();
  expect(game.take()).toMatchObject([
    { kind: "table", preview: null },
    { kind: "table", preview },
  ]);
});
