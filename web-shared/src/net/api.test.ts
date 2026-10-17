import { expect, it } from "vitest";
import {
  ApiError,
  type Fetch,
  PLAYER_SESSION_HEADER,
  createTable,
  gameGone,
  joinLobby,
  lobbyPath,
  lobbyPost,
  mapsPath,
  playerOf,
  postJson,
  tableSocketPath,
} from "./api.ts";

interface Call {
  url: string;
  init: RequestInit | undefined;
}

/** A `fetch` that answers every call with `status` and `body`, and keeps the calls. */
function server(status = 200, body: unknown = {}) {
  const calls: Call[] = [];
  const fetcher = (async (url: string, init?: RequestInit) => {
    calls.push({ url, init });
    const text = typeof body === "string" ? body : JSON.stringify(body);
    return {
      ok: status < 300,
      status,
      text: async () => text,
      json: async () => JSON.parse(text),
    };
  }) as unknown as Fetch;
  return { calls, fetcher };
}

it("the addresses escape the id of a game", () => {
  expect(lobbyPath("a/b")).toBe("/api/games/a%2Fb/lobby");
  expect(lobbyPath("g1", "map-preview")).toBe("/api/games/g1/lobby/map-preview");
  expect(tableSocketPath("a b")).toBe("/ws/tables/a%20b");
  expect(mapsPath(6)).toBe("/api/maps?player_count=6");
});

it("a post sends JSON with the player session, and no body or content type without one", async () => {
  const { calls, fetcher } = server();
  await lobbyPost("g1", "map", { map: { kind: "random" } }, { session: "secret" }, fetcher);
  await postJson("/api/games/g1/terminate", undefined, { session: "secret" }, fetcher);
  await lobbyPost("g1", "start", {}, {}, fetcher);
  expect(calls).toEqual([
    {
      url: "/api/games/g1/lobby/map",
      init: {
        method: "POST",
        headers: { "content-type": "application/json", [PLAYER_SESSION_HEADER]: "secret" },
        body: '{"map":{"kind":"random"}}',
      },
    },
    {
      url: "/api/games/g1/terminate",
      init: { method: "POST", headers: { [PLAYER_SESSION_HEADER]: "secret" } },
    },
    {
      url: "/api/games/g1/lobby/start",
      init: { method: "POST", headers: { "content-type": "application/json" }, body: "{}" },
    },
  ]);
});

it("a join of a new player or a takeover goes with no session, a signal goes along", async () => {
  const { calls, fetcher } = server();
  const signal = new AbortController().signal;
  await joinLobby("g1", { kind: "new", nickname: "Ada" }, { signal }, fetcher);
  await joinLobby("g1", { kind: "takeover", player_id: "bo", nickname: "Bo" }, {}, fetcher);
  expect(calls.map((call) => call.url)).toEqual([
    "/api/games/g1/lobby/join",
    "/api/games/g1/lobby/join",
  ]);
  expect(calls[0].init?.signal).toBe(signal);
  expect(calls.map((call) => call.init?.headers)).toEqual([
    { "content-type": "application/json" },
    { "content-type": "application/json" },
  ]);
  expect(calls.map((call) => JSON.parse(String(call.init?.body)))).toEqual([
    { kind: "new", nickname: "Ada" },
    { kind: "takeover", player_id: "bo", nickname: "Bo" },
  ]);
});

it("a table is made with what the page asks, and a refusal keeps its status and text", async () => {
  const admitted = { game_id: "g9", player_session: "s", player: { id: "p" }, lobby: {} };
  const { calls, fetcher } = server(200, admitted);
  await expect(
    createTable({ playerCount: 6, nickname: "Ada", seed: 3, map: "milty6" }, fetcher),
  ).resolves.toEqual(admitted);
  await createTable(
    { playerCount: 8, nickname: "Ada", map: "random", fill: "random", waitMs: 1000, cards: "pok" },
    fetcher,
  );
  expect(calls.map((call) => call.url)).toEqual(["/api/games", "/api/games"]);
  expect(calls.map((call) => JSON.parse(String(call.init?.body)))).toEqual([
    { player_count: 6, nickname: "Ada", seed: 3, map_template: "milty6" },
    {
      player_count: 8,
      nickname: "Ada",
      fill_seats: "random",
      bot_delay_ms: 1000,
      strategy_card_set: "pok",
      map: "random",
    },
  ]);
  const refused = createTable({ playerCount: 4, nickname: "Ada" }, server(429, "busy").fetcher);
  await expect(refused).rejects.toBeInstanceOf(ApiError);
  await expect(refused).rejects.toMatchObject({ status: 429, body: "busy" });
});

it("the player of a kept session is asked with that session", async () => {
  const known = server(200, { player: { id: "ada" } });
  await expect(playerOf("g1", "secret", known.fetcher)).resolves.toBe("ada");
  expect(known.calls[0].init?.headers).toMatchObject({ [PLAYER_SESSION_HEADER]: "secret" });
  expect(JSON.parse(String(known.calls[0].init?.body))).toEqual({ kind: "new" });
  await expect(playerOf("g1", "secret", server(403).fetcher)).resolves.toBeNull();
});

it("a game is gone only when the server says not found", async () => {
  const gone = server(404, "Game not found");
  await expect(gameGone("g1", gone.fetcher)).resolves.toBe(true);
  expect(gone.calls[0].url).toBe("/api/games/g1/lobby");
  await expect(gameGone("g1", server(200).fetcher)).resolves.toBe(false);
  await expect(gameGone("g1", server(502).fetcher)).resolves.toBe(false);
  const unreachable = (async () => {
    throw new TypeError("fetch failed");
  }) as unknown as Fetch;
  await expect(gameGone("g1", unreachable)).resolves.toBe(false);
});
