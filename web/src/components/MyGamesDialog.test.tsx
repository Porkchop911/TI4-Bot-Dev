import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { CreateLobby } from "./Lobby.tsx";
import { listMyGames, rememberMyGame } from "../protocol/myGames.ts";

afterEach(() => {
  localStorage.clear();
  sessionStorage.clear();
  vi.unstubAllGlobals();
});

const refusal = (games: unknown[]) => ({
  ok: false,
  status: 429,
  text: async (): Promise<string> =>
    JSON.stringify({
      error: "too_many_games",
      message: "you already have several unfinished games on this server",
      games,
    }),
});

const created = {
  game_id: "game",
  player_session: "tok-new",
  player: { id: "player_a" },
  lobby: {
    game_id: "game",
    phase: "lobby",
    lobby_version: 1,
    host_player_id: "player_a",
    slots: [
      {
        slot_id: "slot_1",
        position: 1,
        occupant: "player_a",
        nickname: "Host",
        ready: false,
        connected: true,
        can_take_over: false,
      },
      {
        slot_id: "slot_2",
        position: 2,
        occupant: null,
        nickname: null,
        ready: false,
        connected: false,
        can_take_over: false,
      },
    ],
  },
};

describe("too many games dialog", () => {
  it("lists the open games, confirms, terminates with the stored token, then retries create", async () => {
    rememberMyGame({ gameId: "game_aaaaaa1", token: "tok-a" });
    const now = Math.floor(Date.now() / 1000);
    let createCalls = 0;
    const fetchMock = vi.fn(async (url: string, init?: RequestInit) => {
      if (url === "/api/games" && init?.method === "POST") {
        createCalls++;
        return createCalls === 1
          ? refusal([
              { game_id: "game_aaaaaa1", phase: "lobby", players: 1, created_at: now - 600 },
              { game_id: "game_bbbbbb2", phase: "started", players: 3, created_at: now - 7200 },
            ])
          : { ok: true, status: 200, json: async () => created };
      }
      if (String(url).endsWith("/terminate")) return { ok: true, status: 200, json: async () => ({}) };
      return { ok: true, status: 200, json: async () => ({}), text: async (): Promise<string> => "" };
    });
    vi.stubGlobal("fetch", fetchMock);
    const onCreated = vi.fn();
    const onError = vi.fn();
    render(<CreateLobby onCreated={onCreated} onError={onError} />);
    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "Host" } });
    fireEvent.click(screen.getByTestId("create-game-button"));

    const dialog = await screen.findByTestId("mygames-dialog");
    expect(onError).not.toHaveBeenCalled();
    expect(within(dialog).getByText("You have too many unfinished games")).toBeInTheDocument();
    const rows = within(dialog).getAllByTestId("mygames-row");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent("Lobby · 1 player · 10 min ago");
    expect(rows[1]).toHaveTextContent("In progress · 3 players · 2 h ago");
    // The second game was made in another browser: no token, no Terminate button.
    expect(within(rows[1]).getByTestId("mygames-not-yours")).toHaveTextContent(
      "Not yours (other browser)",
    );
    expect(within(rows[1]).queryByText("Terminate")).toBeNull();
    expect(within(dialog).getByText("Create game")).toBeDisabled();

    fireEvent.click(within(rows[0]).getByText("Terminate"));
    expect(within(rows[0]).getByText("Really end this game?")).toBeInTheDocument();
    fireEvent.click(within(rows[0]).getByText("No"));
    expect(within(rows[0]).queryByText("Really end this game?")).toBeNull();
    expect(fetchMock.mock.calls.some((c) => String(c[0]).endsWith("/terminate"))).toBe(false);

    fireEvent.click(within(rows[0]).getByText("Terminate"));
    fireEvent.click(within(rows[0]).getByText("Yes"));
    await waitFor(() => expect(within(dialog).getAllByTestId("mygames-row")).toHaveLength(1));
    const call = fetchMock.mock.calls.find((c) => String(c[0]).endsWith("/terminate"))!;
    expect(call[0]).toBe("/api/games/game_aaaaaa1/terminate");
    expect((call[1] as RequestInit).headers).toEqual({ "x-ti4-player-session": "tok-a" });
    expect(listMyGames().map((g) => g.gameId)).not.toContain("game_aaaaaa1");

    fireEvent.click(within(dialog).getByText("Create game"));
    await waitFor(() => expect(onCreated).toHaveBeenCalledOnce());
    expect(onCreated.mock.calls[0][0].game_id).toBe("game");
    expect(createCalls).toBe(2);
    expect(screen.queryByTestId("mygames-dialog")).toBeNull();
  });

  it("opens a game from the list with its stored token and ends all of mine at once", async () => {
    rememberMyGame({ gameId: "game_aaaaaa1", token: "tok-a" });
    rememberMyGame({ gameId: "game_bbbbbb2", token: "tok-b" });
    const fetchMock = vi.fn(async (url: string, init?: RequestInit) => {
      if (url === "/api/games" && init?.method === "POST")
        return refusal([
          { game_id: "game_aaaaaa1", phase: "lobby", players: 1 },
          { game_id: "game_bbbbbb2", phase: "lobby", players: 2 },
        ]);
      return { ok: true, status: 200, json: async () => ({}), text: async (): Promise<string> => "" };
    });
    vi.stubGlobal("fetch", fetchMock);
    const onOpenGame = vi.fn();
    render(<CreateLobby onCreated={vi.fn()} onError={vi.fn()} onOpenGame={onOpenGame} />);
    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "Host" } });
    fireEvent.click(screen.getByTestId("create-game-button"));
    const dialog = await screen.findByTestId("mygames-dialog");
    fireEvent.click(within(within(dialog).getAllByTestId("mygames-row")[1]).getByText("Open"));
    expect(onOpenGame).toHaveBeenCalledWith("game_bbbbbb2", "tok-b");

    fireEvent.click(within(dialog).getByText("Terminate all my games"));
    fireEvent.click(within(dialog).getByText("Yes"));
    await waitFor(() => expect(within(dialog).getByTestId("mygames-empty")).toBeInTheDocument());
    const ended = fetchMock.mock.calls.filter((c) => String(c[0]).endsWith("/terminate"));
    expect(ended.map((c) => c[0])).toEqual([
      "/api/games/game_aaaaaa1/terminate",
      "/api/games/game_bbbbbb2/terminate",
    ]);
  });

  it("keeps showing a plain error for other failures and lists stored games from the start screen", async () => {
    const fetchMock = vi.fn(async (url: string, init?: RequestInit) => {
      if (url === "/api/games" && init?.method === "POST")
        return { ok: false, status: 429, text: async (): Promise<string> => '{"error":"rate_limited"}' };
      if (url === "/api/games") return { ok: true, status: 200, json: async () => [{ game_id: "game_live000" }] };
      return { ok: true, status: 200, json: async () => ({}), text: async (): Promise<string> => "" };
    });
    vi.stubGlobal("fetch", fetchMock);
    rememberMyGame({ gameId: "game_live000", token: "t1" });
    rememberMyGame({ gameId: "game_dead000", token: "t2" });
    const onError = vi.fn();
    render(<CreateLobby onCreated={vi.fn()} onError={onError} />);
    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "Host" } });
    fireEvent.click(screen.getByTestId("create-game-button"));
    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect(screen.queryByTestId("mygames-dialog")).toBeNull();

    fireEvent.click(screen.getByTestId("my-games-button"));
    const dialog = await screen.findByTestId("mygames-dialog");
    expect(within(dialog).getByText("My games")).toBeInTheDocument();
    expect(within(dialog).getAllByTestId("mygames-row")).toHaveLength(1);
    expect(listMyGames().map((g) => g.gameId)).toEqual(["game_live000"]);
  });
});
