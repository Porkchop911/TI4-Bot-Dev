import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { App } from "./App.tsx";

const lobby = (phase: "lobby" | "running" = "lobby", canTakeOver = false) => ({
  game_id: "game-1",
  phase,
  lobby_version: 1,
  host_player_id: "player_a",
  slots: [
    {
      slot_id: "slot_1",
      position: 1,
      occupant: "player_a",
      nickname: "Host",
      ready: false,
      connected: false,
      can_take_over: canTakeOver,
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
});
afterEach(() => {
  cleanup();
  sessionStorage.clear();
  localStorage.clear();
  history.replaceState({}, "", "/");
  vi.unstubAllGlobals();
});

describe("App lobby routing", () => {
  it("reads without admission, then joins and retains the credential in tab storage only", async () => {
    history.replaceState({}, "", "/games/game-1");
    const fetchMock = vi.fn().mockImplementation((url: string) =>
      Promise.resolve({
        ok: true,
        json: async () =>
          url.endsWith("/join")
            ? {
                player_session: "private",
                player: { id: "player_b" },
                lobby: {
                  ...lobby(),
                  slots: [
                    lobby().slots[0],
                    { ...lobby().slots[1], occupant: "player_b", nickname: "Guest" },
                  ],
                },
              }
            : lobby(),
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    await waitFor(() => expect(screen.getByText("Join game")).toBeInTheDocument());
    expect(fetchMock.mock.calls.filter(([url]) => String(url).endsWith("/join"))).toHaveLength(0);
    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "Guest" } });
    fireEvent.click(screen.getByText("Join game"));
    await waitFor(() =>
      expect(sessionStorage.getItem("ti4.player-session:game-1")).toBe("private"),
    );
    expect(
      JSON.parse(fetchMock.mock.calls.find(([url]) => String(url).endsWith("/join"))![1].body),
    ).toEqual({ kind: "new", nickname: "Guest" });
    expect(localStorage.getItem("ti4.nickname")).toBe("Guest");
    expect(screen.getByText(/Position 2: Guest/)).toBeInTheDocument();
    expect(window.location.pathname).toBe("/games/game-1");
  });

  it("watches without admitting and reuses the saved nickname for a later join", async () => {
    localStorage.setItem("ti4.nickname", "Visitor");
    history.replaceState({}, "", "/games/game-1");
    const fetchMock = vi.fn().mockImplementation((url: string) =>
      Promise.resolve({
        ok: true,
        json: async () =>
          url.endsWith("/join")
            ? {
                player_session: "private",
                player: { id: "player_b" },
                lobby: {
                  ...lobby(),
                  slots: [
                    lobby().slots[0],
                    { ...lobby().slots[1], occupant: "player_b", nickname: "Visitor" },
                  ],
                },
              }
            : lobby(),
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    fireEvent.click(await screen.findByText("Watch"));
    expect(screen.getByLabelText("Nickname")).toHaveValue("Visitor");
    expect(fetchMock.mock.calls.filter(([url]) => String(url).endsWith("/join"))).toHaveLength(0);
    fireEvent.click(screen.getByText("Join game"));
    await waitFor(() =>
      expect(sessionStorage.getItem("ti4.player-session:game-1")).toBe("private"),
    );
    expect(
      JSON.parse(fetchMock.mock.calls.find(([url]) => String(url).endsWith("/join"))![1].body),
    ).toEqual({ kind: "new", nickname: "Visitor" });
  });

  it("refuses invalid join nicknames locally without consuming an open position", async () => {
    history.replaceState({}, "", "/games/game-1");
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => lobby() });
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    await screen.findByText("Join game");
    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "x".repeat(65) } });
    fireEvent.click(screen.getByText("Join game"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Nickname");
    expect(fetchMock.mock.calls.filter(([url]) => String(url).endsWith("/join"))).toHaveLength(0);
  });

  it("explicitly takes over an eligible disconnected player on a fresh computer", async () => {
    history.replaceState({}, "", "/games/game-1");
    const fetchMock = vi.fn().mockImplementation((url: string) =>
      Promise.resolve({
        ok: true,
        json: async () =>
          url.endsWith("/join")
            ? {
                player_session: "replacement",
                player: { id: "player_a" },
                lobby: {
                  ...lobby(),
                  slots: [{ ...lobby().slots[0], nickname: "New host" }, lobby().slots[1]],
                },
              }
            : lobby("lobby", true),
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    await screen.findByText(/Rejoin as Host/);
    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "New host" } });
    fireEvent.click(screen.getByText(/Rejoin as Host/));
    await waitFor(() =>
      expect(sessionStorage.getItem("ti4.player-session:game-1")).toBe("replacement"),
    );
    expect(
      JSON.parse(fetchMock.mock.calls.find(([url]) => String(url).endsWith("/join"))![1].body),
    ).toEqual({ kind: "takeover", player_id: "player_a", nickname: "New host" });
    expect(screen.getByText(/Position 1: New host/)).toBeInTheDocument();
  });

  it("reconnects with its stored credential and sends a complete slot permutation as host", async () => {
    localStorage.setItem("ti4.nickname", "Different preference");
    sessionStorage.setItem("ti4.player-session:game-1", "private");
    history.replaceState({}, "", "/games/game-1");
    const fetchMock = vi.fn().mockImplementation((url: string) =>
      Promise.resolve({
        ok: true,
        json: async () =>
          url.endsWith("/join")
            ? { player_session: null, player: { id: "player_a" }, lobby: lobby() }
            : lobby(),
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    fireEvent.click(await screen.findByLabelText("Move position 2 up"));
    await waitFor(() =>
      expect(fetchMock.mock.calls.some(([url]) => String(url).endsWith("/reorder"))).toBe(true),
    );
    const reorder = fetchMock.mock.calls.find(([url]) => String(url).endsWith("/reorder"))![1];
    expect(reorder.headers["x-ti4-player-session"]).toBe("private");
    expect(JSON.parse(reorder.body)).toEqual({ slot_ids: ["slot_2", "slot_1"] });
    expect(screen.getByText(/Position 1: Host/)).toBeInTheDocument();
    expect(localStorage.getItem("ti4.nickname")).toBe("Different preference");
    expect(
      JSON.parse(fetchMock.mock.calls.find(([url]) => String(url).endsWith("/join"))![1].body),
    ).toEqual({ kind: "new" });
  });

  it("drops a revoked credential and exposes read-only spectator actions", async () => {
    sessionStorage.setItem("ti4.player-session:game-1", "old");
    history.replaceState({}, "", "/games/game-1");
    const fetchMock = vi
      .fn()
      .mockImplementation((url: string) =>
        Promise.resolve(
          url.endsWith("/join")
            ? { ok: false, status: 403 }
            : { ok: true, json: async () => lobby() },
        ),
      );
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    await waitFor(() => expect(sessionStorage.getItem("ti4.player-session:game-1")).toBeNull());
    expect(await screen.findByText("Watch")).toBeInTheDocument();
    expect(window.location.pathname).toBe("/games/game-1");
  });

  it("leaves an unstarted lobby on the server before forgetting the guest credential", async () => {
    sessionStorage.setItem("ti4.player-session:game-1", "guest-session");
    history.replaceState({}, "", "/games/game-1");
    const guestLobby = {
      ...lobby(),
      slots: [lobby().slots[0], { ...lobby().slots[1], occupant: "player_b", nickname: "Guest" }],
    };
    const fetchMock = vi.fn().mockImplementation((url: string) =>
      Promise.resolve({
        ok: true,
        json: async () =>
          url.endsWith("/join")
            ? { player_session: null, player: { id: "player_b" }, lobby: guestLobby }
            : url.endsWith("/leave")
              ? lobby()
              : guestLobby,
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    fireEvent.click(await screen.findByText("Leave lobby"));
    await waitFor(() => expect(window.location.pathname).toBe("/"));
    expect(sessionStorage.getItem("ti4.player-session:game-1")).toBeNull();
    const leaveCall = fetchMock.mock.calls.find(([url]) => String(url).endsWith("/leave"))!;
    expect(leaveCall[1]).toMatchObject({
      method: "POST",
      headers: { "x-ti4-player-session": "guest-session" },
    });
  });

  it("retains the guest credential and shows an error if leaving fails", async () => {
    sessionStorage.setItem("ti4.player-session:game-1", "guest-session");
    history.replaceState({}, "", "/games/game-1");
    const guestLobby = {
      ...lobby(),
      slots: [lobby().slots[0], { ...lobby().slots[1], occupant: "player_b", nickname: "Guest" }],
    };
    const fetchMock = vi.fn().mockImplementation((url: string) =>
      Promise.resolve(
        url.endsWith("/leave")
          ? { ok: false, status: 500, text: async () => "Storage error" }
          : {
              ok: true,
              json: async () =>
                url.endsWith("/join")
                  ? { player_session: null, player: { id: "player_b" }, lobby: guestLobby }
                  : guestLobby,
            },
      ),
    );
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    fireEvent.click(await screen.findByText("Leave lobby"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Leave lobby failed (500)");
    expect(sessionStorage.getItem("ti4.player-session:game-1")).toBe("guest-session");
    expect(window.location.pathname).toBe("/games/game-1");
  });
});
