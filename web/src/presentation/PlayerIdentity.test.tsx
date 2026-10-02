import { expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { PlayerIdentityProvider, SeatBadge } from "./PlayerIdentity.tsx";
import { PlayerSheet } from "../components/PlayerSheet.tsx";
import { TurnStatusBar } from "../components/TurnStatusBar.tsx";
import { EventLog } from "../components/EventLog.tsx";
import { Board } from "../components/Board.tsx";
import { TradeDeskModal } from "../components/TradeDeskModal.tsx";
import type { LobbyDto, PlayerView, GameView } from "../protocol/types.ts";

const slots: LobbyDto["slots"] = [
  {
    slot_id: "slot_2",
    position: 1,
    occupant: "player_b",
    nickname: "Sam",
    ready: true,
    connected: true,
    can_take_over: false,
  },
  {
    slot_id: "slot_1",
    position: 2,
    occupant: "player_a",
    nickname: "Sam",
    ready: true,
    connected: true,
    can_take_over: false,
  },
];
const lobby: LobbyDto = {
  game_id: "g",
  phase: "running",
  lobby_version: 2,
  host_player_id: "player_a",
  slots,
};
const players = ["player_b", "player_a"].map((id) => ({
  id,
  faction: "sol",
  victory_points: 1,
  trade_goods: 0,
  commodities: 0,
  tactic_tokens: 0,
  fleet_tokens: 0,
  strategic_tokens: 0,
  passed: false,
  strategy_cards: [],
  exhausted_strategy_cards: [],
  technologies: [],
  exhausted_technologies: [],
  relics: [],
  exhausted_relics: [],
  action_cards_count: 0,
  secret_objectives_count: 0,
  leaders: {},
})) satisfies PlayerView[];
const board: GameView["board"] = {
  systems: {
    "18": {
      system_id: "18",
      command_tokens: ["player_b"],
      planets: { mecatol: { planet_id: "mecatol", controlled_by: "player_a", exhausted: false } },
      units: [{ unit_type: "infantry", owner: "player_a", damaged: false }],
    },
  },
  map_tiles: [
    {
      system_id: "18",
      label: "Mecatol",
      q: 0,
      r: 0,
      planets: [{ id: "mecatol", label: "Mecatol", resources: 1, influence: 6 }],
    },
  ],
};
const view: GameView = {
  round: 1,
  phase: "action",
  speaker: "player_b",
  seating_order: ["player_b", "player_a"],
  players,
  board,
  table: {
    revealed_objectives: [],
    scored_objectives: {},
    unclaimed_strategy_cards: [],
    strategy_card_goods: {},
    laws: {},
  },
  finished: false,
};

it("renders player and spectator surfaces with current names, seat shapes and no identity attributes; updates after takeover", () => {
  const surfaces = (roster: LobbyDto) => (
    <PlayerIdentityProvider lobby={roster} seatingOrder={view.seating_order}>
      <TurnStatusBar
        status={{ kind: "active_turn", player: "player_a", phase: "action", round: 1 }}
        view={view}
        gameVersion={3}
        connectionStatus="connected"
      />
      <PlayerSheet players={players} userSeat="player_b" />
      <Board board={board} seatingOrder={view.seating_order} players={players} />
      <EventLog
        events={[
          {
            id: "e1",
            timestamp: "",
            visibility: "public",
            event: { kind: "game_finished", winner: "player_a" },
          },
        ]}
        isOpen
        onToggle={vi.fn()}
      />
    </PlayerIdentityProvider>
  );
  const { container, rerender } = render(surfaces(lobby));
  fireEvent.click(screen.getByRole("button", { name: /Unknown round/ }));
  fireEvent.click(screen.getByRole("button", { name: /Unknown phase/ }));
  expect(screen.getByText(/Active Turn: Sam \(▲ Position 2\)/)).toBeInTheDocument();
  expect(screen.getByText(/Game finished: Sam \(▲ Position 2\) wins/)).toBeInTheDocument();
  expect(screen.getByLabelText("Player positions")).toHaveTextContent("Sam (● Position 1)");
  expect(container.querySelector('[data-private-card-owner="player_b"]')).toBeNull();
  expect(container.outerHTML).not.toContain("player_a");
  expect(container.outerHTML).not.toContain("player_b");
  const renamed = {
    ...lobby,
    slots: slots.map((slot) =>
      slot.occupant === "player_a" ? { ...slot, nickname: "Robin" } : slot,
    ),
  };
  rerender(surfaces(renamed));
  expect(screen.getByText(/Active Turn: Robin/)).toBeInTheDocument();
  expect(screen.getByText(/Game finished: Robin wins/)).toBeInTheDocument();
  expect(container.outerHTML).not.toContain("player_a");
  // Spectator uses the same public roster, but receives no private hand.
  const spectator = render(
    <PlayerIdentityProvider lobby={renamed} seatingOrder={view.seating_order}>
      <TurnStatusBar
        status={{
          kind: "waiting_for_decision",
          seat: "player_a",
          phase: "action",
          round: 1,
          stage: "choice",
        }}
        view={view}
        gameVersion={3}
        connectionStatus="connected"
      />
      <PlayerSheet players={players} />
    </PlayerIdentityProvider>,
  );
  expect(screen.getByText(/Waiting for Robin/)).toBeInTheDocument();
  expect(spectator.container.querySelector("[data-private-card]")).toBeNull();
  expect(spectator.container.outerHTML).not.toContain("player_a");
  // The black eighth position has a bright outline and white glyph even on a dark canvas.
  const { container: badge } = render(<SeatBadge position={8} />);
  expect(badge.querySelector("circle")).toHaveAttribute("fill", "#000000");
  expect(badge.querySelector("text")).toHaveAttribute("fill", "#fff");
  fireEvent.click(screen.getByTestId("event-log-toggle"));
});

it("keeps trade target and submitted choice IDs internal while displaying the current roster", async () => {
  const onSubmit = vi.fn().mockResolvedValue(undefined);
  const choice = {
    actor: "player_b",
    nonce: "1",
    prompt: "Propose trade",
    context: { subtype: "propose_transaction", target: { Player: "player_a" } },
    options: [{ id: "offer-1", label: "Exchange commodities", kind: "offer" }],
  };
  const { container, rerender } = render(
    <PlayerIdentityProvider lobby={lobby} seatingOrder={view.seating_order}>
      <TradeDeskModal
        choice={choice}
        viewerSeat="player_b"
        onSubmit={onSubmit}
        isOpen
        onClose={vi.fn()}
      />
    </PlayerIdentityProvider>,
  );
  expect(screen.getByTestId("trade-desk-title")).toHaveTextContent("Propose a trade");
  expect(screen.getByText("With Sam (▲ Position 2)")).toBeInTheDocument();
  expect(container.textContent).not.toContain("player_a");
  rerender(
    <PlayerIdentityProvider
      lobby={{
        ...lobby,
        slots: slots.map((slot) =>
          slot.occupant === "player_a" ? { ...slot, nickname: "Robin" } : slot,
        ),
      }}
      seatingOrder={view.seating_order}
    >
      <TradeDeskModal
        choice={choice}
        viewerSeat="player_b"
        onSubmit={onSubmit}
        isOpen
        onClose={vi.fn()}
      />
    </PlayerIdentityProvider>,
  );
  expect(screen.getByText("With Robin")).toBeInTheDocument();
  fireEvent.click(screen.getByTestId("trade-opt-offer-1"));
  await act(async () => {
    fireEvent.click(screen.getByTestId("propose-trade-btn"));
  });
  expect(onSubmit).toHaveBeenCalledWith("offer-1");
});
