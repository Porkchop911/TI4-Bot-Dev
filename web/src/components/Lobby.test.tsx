import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { CreateLobby, LobbyStatus } from "./Lobby.tsx";
import { LobbyDto } from "../protocol/types.ts";

const lobby: LobbyDto = {
  game_id: "game",
  phase: "lobby",
  lobby_version: 2,
  host_player_id: "player_a",
  slots: [
    {
      slot_id: "slot_1",
      position: 1,
      occupant: "player_a",
      nickname: "Same name",
      connected: true,
      can_take_over: false,
    },
    {
      slot_id: "slot_2",
      position: 2,
      occupant: "player_b",
      nickname: "Same name",
      connected: false,
      can_take_over: true,
    },
    {
      slot_id: "slot_3",
      position: 3,
      occupant: null,
      nickname: null,
      connected: false,
      can_take_over: false,
    },
  ],
};
const props = {
  onStart: vi.fn(),
  onLeave: vi.fn(),
  onJoin: vi.fn(),
  onWatch: vi.fn(),
  onTakeover: vi.fn(),
};
afterEach(() => {
  localStorage.clear();
  vi.unstubAllGlobals();
});

describe("lobby UI", () => {
  it("creates empty positions without choosing bot or player identities", async () => {
    const onCreated = vi.fn();
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        game_id: "game",
        player_session: "secret",
        player: { id: "player_a" },
        lobby,
      }),
    });
    vi.stubGlobal("fetch", fetchMock);
    render(<CreateLobby onCreated={onCreated} onError={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "Host 🪐" } });
    fireEvent.change(screen.getByLabelText("Players"), { target: { value: "3" } });
    fireEvent.click(screen.getByTestId("create-game-button"));
    await waitFor(() => expect(onCreated).toHaveBeenCalledOnce());
    // The Alpha notice also asks /health once; pick the create call.
    const createCall = fetchMock.mock.calls.find((call) => call[0] === "/api/games")!;
    expect(JSON.parse(createCall[1].body)).toEqual({
      player_count: 3,
      nickname: "Host 🪐",
      strategy_card_set: "te",
    });
    expect(localStorage.getItem("ti4.nickname")).toBe("Host 🪐");
  });

  it("offers the strategy card sets with Thunder's Edge preselected and sends the choice", async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: false, status: 500, text: async () => "x" });
    vi.stubGlobal("fetch", fetchMock);
    render(<CreateLobby onCreated={vi.fn()} onError={vi.fn()} />);
    const select = screen.getByLabelText("Strategy cards");
    expect(select).toHaveValue("te");
    expect(screen.getByRole("option", { name: "Thunder's Edge (default)" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Prophecy of Kings" })).toBeInTheDocument();
    expect(screen.getByTestId("strategy-card-set-description")).toHaveTextContent(/Thunder's Edge/);
    fireEvent.change(select, { target: { value: "pok" } });
    expect(screen.getByTestId("strategy-card-set-description")).toHaveTextContent(/Prophecy of Kings/);
    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "Host" } });
    fireEvent.click(screen.getByTestId("create-game-button"));
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    expect(JSON.parse(fetchMock.mock.calls[0][1].body).strategy_card_set).toBe("pok");
  });

  it("names the table's strategy card set in the lobby, and nothing for an older server", () => {
    const { rerender } = render(
      <LobbyStatus lobby={{ ...lobby, strategy_card_set: "te" }} playerId={null} {...props} />,
    );
    expect(screen.getByTestId("lobby-card-set-row")).toHaveTextContent("Strategy cards: Thunder's Edge");
    rerender(<LobbyStatus lobby={{ ...lobby, strategy_card_set: "pok" }} playerId={null} {...props} />);
    expect(screen.getByTestId("lobby-card-set-row")).toHaveTextContent("Prophecy of Kings");
    rerender(<LobbyStatus lobby={lobby} playerId={null} {...props} />);
    expect(screen.queryByTestId("lobby-card-set-row")).toBeNull();
  });

  it("hides the seed input outside dev builds and never sends a seed", async () => {
    vi.stubEnv("DEV", false);
    const fetchMock = vi.fn().mockResolvedValue({ ok: false, status: 500, text: async () => "x" });
    vi.stubGlobal("fetch", fetchMock);
    render(<CreateLobby onCreated={vi.fn()} onError={vi.fn()} />);
    expect(screen.queryByLabelText(/Seed/)).toBeNull();
    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "Host" } });
    fireEvent.click(screen.getByTestId("create-game-button"));
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    expect(JSON.parse(fetchMock.mock.calls[0][1].body)).not.toHaveProperty("seed");
    vi.unstubAllEnvs();
  });

  it("shows the seed input behind the dev flag and sends the typed seed", async () => {
    vi.stubEnv("DEV", true);
    const fetchMock = vi.fn().mockResolvedValue({ ok: false, status: 500, text: async () => "x" });
    vi.stubGlobal("fetch", fetchMock);
    render(<CreateLobby onCreated={vi.fn()} onError={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "Host" } });
    fireEvent.change(screen.getByLabelText(/Seed/), { target: { value: "42" } });
    fireEvent.click(screen.getByTestId("create-game-button"));
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    expect(JSON.parse(fetchMock.mock.calls[0][1].body).seed).toBe(42);
    vi.unstubAllEnvs();
  });

  it("prefills the saved preference and refuses invalid creation before any request", async () => {
    localStorage.setItem("ti4.nickname", "Returning visitor");
    const fetchMock = vi.fn();
    const onError = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    render(<CreateLobby onCreated={vi.fn()} onError={onError} />);
    expect(screen.getByLabelText("Nickname")).toHaveValue("Returning visitor");
    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: " bad " } });
    fireEvent.click(screen.getByText("Create lobby"));
    expect(onError).toHaveBeenCalledWith(expect.stringContaining("Nickname"));
    expect(fetchMock).not.toHaveBeenCalled();
    expect(localStorage.getItem("ti4.nickname")).toBe("Returning visitor");
  });

  it("offers a way back to the start screen without leaving a seat (fb_1791875585)", () => {
    const onHome = vi.fn();
    const { rerender } = render(<LobbyStatus lobby={{ ...lobby, phase: "running" }} playerId={null} {...props} onHome={onHome} />);
    fireEvent.click(screen.getByTestId("lobby-back-to-start"));
    expect(onHome).toHaveBeenCalledOnce();
    expect(props.onLeave).not.toHaveBeenCalled();
    rerender(<LobbyStatus lobby={lobby} playerId="player_a" {...props} onHome={onHome} />);
    expect(screen.getByTestId("lobby-back-to-start")).toHaveTextContent("Back to start");
  });

  it("shows watch, first-open join, and eligible explicit rejoin without admitting on render", () => {
    render(<LobbyStatus lobby={lobby} playerId={null} {...props} />);
    fireEvent.click(screen.getByText("Watch"));
    fireEvent.click(screen.getByText("Join game"));
    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "Replacement" } });
    fireEvent.click(screen.getByText(/Rejoin as Same name/));
    expect(props.onWatch).toHaveBeenCalledOnce();
    expect(props.onJoin).toHaveBeenCalledWith("");
    expect(props.onTakeover).toHaveBeenCalledWith("player_b", "Replacement");
    expect(screen.getByText(/Position 3: Open/)).toBeInTheDocument();
    expect(screen.getByLabelText("Position 3").querySelector("svg")).toBeInTheDocument();
    expect(screen.getByText(/Position 1: Same name \(Host\)/)).toBeInTheDocument();
    expect(screen.getByTestId("lobby-container").textContent).not.toContain("player_a");
    expect(screen.getByTestId("lobby-container").textContent).not.toContain("player_b");
    expect(screen.getByTestId("lobby-container").textContent).not.toContain("Game: game");
  });

  it("gives no seat a Ready button and no host a control to move a seat", () => {
    const { rerender } = render(<LobbyStatus lobby={lobby} playerId="player_a" {...props} />);
    expect(screen.queryByLabelText(/Move position/)).toBeNull();
    expect(screen.queryByTestId("ready-button")).toBeNull();
    expect(screen.getByTestId("start-game-button")).toBeDisabled();
    expect(screen.queryByText("Leave lobby")).toBeNull();
    rerender(<LobbyStatus lobby={lobby} playerId="player_b" {...props} />);
    expect(screen.queryByTestId("ready-button")).toBeNull();
    fireEvent.click(screen.getByText("Leave lobby"));
    expect(props.onLeave).toHaveBeenCalledOnce();
    expect(screen.getByText(/Position 1: Same name \(Host\)/)).toBeInTheDocument();
    expect(screen.getByText(/Position 2: Same name/)).toBeInTheDocument();
  });

  it("keeps unjoined watchers and running spectators away from host controls", () => {
    const { rerender } = render(<LobbyStatus lobby={lobby} playerId={null} watching {...props} />);
    expect(screen.queryByTestId("start-game-button")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Join game" })).toHaveClass("button");
    expect(screen.getByRole("button", { name: /Rejoin as/ })).toHaveClass("button");
    expect(screen.getByText("◇ Disconnected")).toHaveClass("lobby-presence--disconnected");
    rerender(
      <LobbyStatus lobby={{ ...lobby, phase: "running" }} playerId={null} watching {...props} />,
    );
    expect(screen.queryByRole("button", { name: "Join game" })).not.toBeInTheDocument();
    expect(screen.queryByTestId("start-game-button")).not.toBeInTheDocument();
  });

  it("explains disabled start, starts a full table, and marks presence without color", () => {
    const { rerender } = render(<LobbyStatus lobby={lobby} playerId="player_a" {...props} />);
    expect(screen.getByTestId("start-game-button")).toBeDisabled();
    expect(screen.getByTestId("start-game-button")).toHaveAttribute(
      "aria-describedby",
      "start-reason",
    );
    expect(screen.getByText("Waiting for 1 open position to be filled.")).toBeInTheDocument();
    expect(screen.getByText("● Connected")).toBeInTheDocument();
    expect(screen.getByText("◇ Disconnected")).toBeInTheDocument();
    const filled = {
      ...lobby,
      slots: [
        lobby.slots[0],
        lobby.slots[1],
        { ...lobby.slots[2], occupant: "player_c", nickname: "C" },
      ],
    };
    // Every seat is filled: nobody has to say Ready.
    rerender(<LobbyStatus lobby={filled} playerId="player_a" {...props} />);
    expect(screen.getByTestId("start-game-button")).toBeEnabled();
  });

  it("locks conflicting actions during a request", () => {
    render(<LobbyStatus lobby={lobby} playerId="player_a" pendingAction="map" {...props} />);
    expect(screen.getByTestId("start-game-button")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Copy game URL" })).toBeDisabled();
  });

  it("reports clipboard failure with a usable fallback", async () => {
    vi.stubGlobal("navigator", {
      clipboard: { writeText: vi.fn().mockRejectedValue(new Error("denied")) },
    });
    render(<LobbyStatus lobby={lobby} playerId={null} {...props} />);
    fireEvent.click(screen.getByRole("button", { name: "Copy game URL" }));
    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent(
        "Copy the address from your browser instead.",
      ),
    );
  });

  it("disables create controls and ignores repeated submits while the request is pending", async () => {
    let resolve!: (value: unknown) => void;
    const fetchMock = vi.fn().mockImplementation(
      () =>
        new Promise((done) => {
          resolve = done;
        }),
    );
    vi.stubGlobal("fetch", fetchMock);
    render(<CreateLobby onCreated={vi.fn()} onError={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "Host" } });
    fireEvent.click(screen.getByTestId("create-game-button"));
    expect(screen.getByTestId("create-game-button")).toBeDisabled();
    expect(screen.getByLabelText("Nickname")).toBeDisabled();
    fireEvent.submit(screen.getByTestId("create-game-button").closest("form")!);
    expect(fetchMock).toHaveBeenCalledOnce();
    resolve({ ok: false, status: 503, text: async () => "Unavailable" });
    await waitFor(() => expect(screen.getByTestId("create-game-button")).toBeEnabled());
  });

  it("allows host to remove an occupied non-host seat", () => {
    const onRemoveBot = vi.fn().mockResolvedValue(true);
    render(<LobbyStatus lobby={lobby} playerId="player_a" onRemoveBot={onRemoveBot} {...props} />);

    const removeBtn = screen.getByTestId("remove-bot-button-2");
    expect(removeBtn).toBeInTheDocument();
    fireEvent.click(removeBtn);
    expect(onRemoveBot).toHaveBeenCalledWith("player_b");
  });
  describe("bots", () => {
    const withBots: LobbyDto = { ...lobby, bot_kinds: ["random", "smart"] };
    const withFactions: LobbyDto = {
      ...withBots,
      factions: ["sol", "hacan", "letnev", "xxcha", "jolnar", "l1z1x"],
    };

    it("gives the host add and fill buttons for the rule bot only, and nobody else", () => {
      const onAddRandomBots = vi.fn().mockResolvedValue(true);
      const { rerender } = render(
        <LobbyStatus lobby={withBots} playerId={null} onAddRandomBots={onAddRandomBots} {...props} />,
      );
      expect(screen.queryByTestId("add-smart-bot-button")).not.toBeInTheDocument();
      rerender(
        <LobbyStatus lobby={withBots} playerId="player_b" onAddRandomBots={onAddRandomBots} {...props} />,
      );
      expect(screen.queryByTestId("add-smart-bot-button")).not.toBeInTheDocument();
      rerender(
        <LobbyStatus lobby={withBots} playerId="player_a" onAddRandomBots={onAddRandomBots} {...props} />,
      );
      fireEvent.click(screen.getByTestId("add-smart-bot-button"));
      expect(onAddRandomBots).toHaveBeenLastCalledWith({ count: 1, kind: "smart" });
      fireEvent.click(screen.getByTestId("fill-smart-bots-button"));
      expect(onAddRandomBots).toHaveBeenLastCalledWith({ fill: true, kind: "smart" });
      // The random bot is not offered.
      expect(screen.queryByTestId("add-random-bot-button")).not.toBeInTheDocument();
      expect(screen.queryByTestId("fill-random-bots-button")).not.toBeInTheDocument();
      expect(screen.queryByText(/random bot/i)).not.toBeInTheDocument();
      // A server without the faction pool offers no picker.
      expect(screen.queryByTestId("new-bot-faction")).not.toBeInTheDocument();
    });

    it("offers nothing on a server that lists no rule bot, or when no seat is open, or after start", () => {
      const onAddRandomBots = vi.fn();
      const { rerender } = render(
        <LobbyStatus
          lobby={{ ...lobby, bot_kinds: ["random"] }}
          playerId="player_a"
          onAddRandomBots={onAddRandomBots}
          {...props}
        />,
      );
      expect(screen.queryByTestId("add-smart-bot-button")).not.toBeInTheDocument();
      const full: LobbyDto = {
        ...withBots,
        slots: withBots.slots.map((slot, i) =>
          slot.occupant ? slot : { ...slot, occupant: `player_${i}`, nickname: "X", ready: true },
        ),
      };
      rerender(
        <LobbyStatus lobby={full} playerId="player_a" onAddRandomBots={onAddRandomBots} {...props} />,
      );
      expect(screen.queryByTestId("fill-smart-bots-button")).not.toBeInTheDocument();
      rerender(
        <LobbyStatus
          lobby={{ ...withBots, phase: "running" }}
          playerId="player_a"
          onAddRandomBots={onAddRandomBots}
          {...props}
        />,
      );
      expect(screen.queryByTestId("add-smart-bot-button")).not.toBeInTheDocument();
    });

    it("adds a bot with the chosen faction, then goes back to Random", async () => {
      const onAddRandomBots = vi.fn().mockResolvedValue(true);
      render(
        <LobbyStatus lobby={withFactions} playerId="player_a" onAddRandomBots={onAddRandomBots} {...props} />,
      );
      expect(screen.getByTestId("new-bot-faction-button")).toHaveTextContent("Random");
      // Closed: no list; open: Random plus every faction by printed name.
      expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
      fireEvent.click(screen.getByTestId("new-bot-faction-button"));
      expect(screen.getAllByRole("option")).toHaveLength(7);
      expect(screen.getByTestId("new-bot-faction-option-letnev")).toHaveTextContent("Barony of Letnev");
      fireEvent.click(screen.getByTestId("new-bot-faction-option-xxcha"));
      expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
      expect(screen.getByTestId("new-bot-faction-button")).toHaveTextContent("Xxcha");
      fireEvent.click(screen.getByTestId("add-smart-bot-button"));
      expect(onAddRandomBots).toHaveBeenLastCalledWith({ count: 1, kind: "smart", faction: "xxcha" });
      await waitFor(() =>
        expect(screen.getByTestId("new-bot-faction-button")).toHaveTextContent("Random"),
      );
      // Fill never carries a faction.
      fireEvent.click(screen.getByTestId("fill-smart-bots-button"));
      expect(onAddRandomBots).toHaveBeenLastCalledWith({ fill: true, kind: "smart" });
    });

    it("disables a faction another seat holds, naming the seat", () => {
      const held: LobbyDto = {
        ...withFactions,
        slots: withFactions.slots.map((slot) =>
          slot.position === 2
            ? { ...slot, nickname: "Rule bot 1", bot: "smart", faction: "sol", connected: true, can_take_over: false }
            : slot,
        ),
      };
      const onAddRandomBots = vi.fn();
      render(<LobbyStatus lobby={held} playerId="player_a" onAddRandomBots={onAddRandomBots} {...props} />);
      fireEvent.click(screen.getByTestId("new-bot-faction-button"));
      const sol = screen.getByTestId("new-bot-faction-option-sol");
      expect(sol).toHaveAttribute("aria-disabled", "true");
      expect(sol).toHaveTextContent("taken by Rule bot 1");
      fireEvent.click(sol);
      expect(screen.getByTestId("new-bot-faction-button")).toHaveTextContent("Random");
    });

    it("lists the chosen factions and lets the host change a bot seat's faction", () => {
      const onSetSeatFaction = vi.fn().mockResolvedValue(true);
      const botLobby: LobbyDto = {
        ...withFactions,
        slots: withFactions.slots.map((slot) =>
          slot.position === 2
            ? { ...slot, nickname: "Rule bot 1", bot: "smart", faction: "hacan", connected: true, can_take_over: false }
            : slot,
        ),
      };
      const { rerender } = render(
        <LobbyStatus lobby={botLobby} playerId="player_a" onSetSeatFaction={onSetSeatFaction} {...props} />,
      );
      expect(screen.getByTestId("seat-faction-picker-2-button")).toHaveTextContent("Emirates of Hacan");
      fireEvent.click(screen.getByTestId("seat-faction-picker-2-button"));
      fireEvent.click(screen.getByTestId("seat-faction-picker-2-option-jolnar"));
      expect(onSetSeatFaction).toHaveBeenLastCalledWith("player_b", "jolnar");
      fireEvent.click(screen.getByTestId("seat-faction-picker-2-button"));
      fireEvent.click(screen.getByTestId("seat-faction-picker-2-option-random"));
      expect(onSetSeatFaction).toHaveBeenLastCalledWith("player_b", null);
      // Others see it as text; a bot with no choice reads Random faction.
      rerender(<LobbyStatus lobby={botLobby} playerId="player_c" {...props} />);
      expect(screen.queryByTestId("seat-faction-picker-2-button")).not.toBeInTheDocument();
      expect(screen.getByTestId("seat-faction-2")).toHaveTextContent("Emirates of Hacan");
      rerender(
        <LobbyStatus
          lobby={{ ...botLobby, slots: botLobby.slots.map((s) => (s.faction ? { ...s, faction: undefined } : s)) }}
          playerId="player_c"
          {...props}
        />,
      );
      expect(screen.getByTestId("seat-faction-2")).toHaveTextContent("Random faction");
    });

    it("labels a rule bot seat as a rule bot", () => {
      const botLobby: LobbyDto = {
        ...withBots,
        slots: withBots.slots.map((slot) =>
          slot.position === 2
            ? { ...slot, nickname: "Rule bot 1", connected: true, can_take_over: false, bot: "smart" }
            : slot,
        ),
      };
      render(<LobbyStatus lobby={botLobby} playerId="player_a" {...props} />);
      expect(screen.getByTestId("bot-badge-2")).toHaveTextContent("Rule bot");
      expect(screen.getByText("● Plays itself")).toBeInTheDocument();
    });

    it("labels a bot seat, says it plays itself, and lets the host remove it", () => {
      const onRemoveBot = vi.fn().mockResolvedValue(true);
      const botLobby: LobbyDto = {
        ...withBots,
        slots: withBots.slots.map((slot) =>
          slot.position === 2
            ? { ...slot, nickname: "Random bot 1", connected: true, can_take_over: false, bot: "random" }
            : slot,
        ),
      };
      render(
        <LobbyStatus lobby={botLobby} playerId="player_a" onRemoveBot={onRemoveBot} {...props} />,
      );
      expect(screen.getByTestId("bot-badge-2")).toHaveTextContent("Random bot");
      expect(screen.queryByTestId("bot-badge-1")).not.toBeInTheDocument();
      expect(screen.getByText("● Plays itself")).toBeInTheDocument();
      const remove = screen.getByRole("button", { name: "Remove bot at position 2" });
      fireEvent.click(remove);
      expect(onRemoveBot).toHaveBeenCalledWith("player_b");
    });
  });

  describe("seat factions", () => {
    const POOL = [
      "sol", "hacan", "letnev", "xxcha", "jolnar", "l1z1x", "arborec", "mentak", "muaat", "naalu",
      "nekro", "saar", "sardakk", "winnu", "yin", "yssaril", "ghost", "argent", "cabal", "empyrean",
      "mahact", "naaz", "nomad", "titans",
    ];
    const wide: LobbyDto = { ...lobby, bot_kinds: ["smart"], factions: POOL };

    it("defaults the create dialog to four players", async () => {
      const fetchMock = vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({ game_id: "g", player_session: "s", player: { id: "player_a" }, lobby }),
      });
      vi.stubGlobal("fetch", fetchMock);
      render(<CreateLobby onCreated={vi.fn()} onError={vi.fn()} />);
      fireEvent.change(screen.getByLabelText("Nickname"), { target: { value: "Host" } });
      expect(screen.getByLabelText("Players")).toHaveValue("4");
      fireEvent.click(screen.getByTestId("create-game-button"));
      await waitFor(() => expect(fetchMock.mock.calls.some((c) => c[0] === "/api/games")).toBe(true));
      const call = fetchMock.mock.calls.find((c) => c[0] === "/api/games")!;
      expect(JSON.parse(call[1].body).player_count).toBe(4);
    });

    it("lists all 24 factions grouped base game / Prophecy of Kings and filters by search", () => {
      render(<LobbyStatus lobby={wide} playerId="player_a" onSetSeatFaction={vi.fn()} {...props} />);
      fireEvent.click(screen.getByTestId("seat-faction-picker-1-button"));
      expect(screen.getAllByRole("option")).toHaveLength(25);
      expect(screen.getByText("Base game")).toBeInTheDocument();
      expect(screen.getByText("Prophecy of Kings")).toBeInTheDocument();
      expect(screen.getByTestId("seat-faction-picker-1-option-titans")).toHaveTextContent("UT");
      fireEvent.change(screen.getByTestId("seat-faction-picker-1-search"), {
        target: { value: "nek" },
      });
      expect(screen.getAllByRole("option")).toHaveLength(1);
      expect(screen.getByTestId("seat-faction-picker-1-option-nekro")).toBeInTheDocument();
      expect(screen.queryByText("Prophecy of Kings")).not.toBeInTheDocument();
    });

    it("lets a non-host person choose their own seat only", () => {
      const onSetSeatFaction = vi.fn().mockResolvedValue(true);
      render(
        <LobbyStatus lobby={wide} playerId="player_b" onSetSeatFaction={onSetSeatFaction} {...props} />,
      );
      expect(screen.queryByTestId("seat-faction-picker-1-button")).not.toBeInTheDocument();
      fireEvent.click(screen.getByTestId("seat-faction-picker-2-button"));
      fireEvent.click(screen.getByTestId("seat-faction-picker-2-option-mahact"));
      expect(onSetSeatFaction).toHaveBeenCalledWith("player_b", "mahact");
    });

    it("lets the host choose for another person and greys what a seat holds", () => {
      const onSetSeatFaction = vi.fn().mockResolvedValue(true);
      const held: LobbyDto = {
        ...wide,
        slots: wide.slots.map((slot) =>
          slot.position === 1 ? { ...slot, faction: "nomad" } : slot,
        ),
      };
      render(
        <LobbyStatus lobby={held} playerId="player_a" onSetSeatFaction={onSetSeatFaction} {...props} />,
      );
      fireEvent.click(screen.getByTestId("seat-faction-picker-2-button"));
      const nomad = screen.getByTestId("seat-faction-picker-2-option-nomad");
      expect(nomad).toHaveAttribute("aria-disabled", "true");
      expect(nomad).toHaveTextContent("taken by Same name");
      fireEvent.click(nomad);
      expect(onSetSeatFaction).not.toHaveBeenCalled();
      fireEvent.click(screen.getByTestId("seat-faction-picker-2-option-cabal"));
      expect(onSetSeatFaction).toHaveBeenCalledWith("player_b", "cabal");
    });

    it("shows everyone's choice to a watcher and offers no picker after the start", () => {
      const chosen: LobbyDto = {
        ...wide,
        slots: wide.slots.map((slot) => (slot.position === 2 ? { ...slot, faction: "yin" } : slot)),
      };
      const { rerender } = render(
        <LobbyStatus lobby={chosen} playerId={null} onSetSeatFaction={vi.fn()} {...props} />,
      );
      expect(screen.getByTestId("seat-faction-2")).toHaveTextContent("Yin");
      expect(screen.queryByTestId("seat-faction-picker-2-button")).not.toBeInTheDocument();
      rerender(
        <LobbyStatus lobby={{ ...chosen, phase: "running" }} playerId="player_a" onSetSeatFaction={vi.fn()} {...props} />,
      );
      expect(screen.queryByTestId("seat-faction-picker-2-button")).not.toBeInTheDocument();
    });
  });
});
