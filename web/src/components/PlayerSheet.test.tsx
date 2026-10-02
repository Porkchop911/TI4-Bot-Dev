import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { PlayerSheet } from "./PlayerSheet.tsx";
import { CardDetails } from "./CardDetails.tsx";
import { PlayerView } from "../protocol/types.ts";

const mockPlayers: PlayerView[] = [
  {
    id: "p1",
    faction: "Federation of Sol",
    victory_points: 3,
    trade_goods: 2,
    commodities: 4,
    tactic_tokens: 3,
    fleet_tokens: 3,
    strategic_tokens: 2,
    passed: false,
    strategy_cards: ["pok1leadership", "pok6warfare"],
    exhausted_strategy_cards: [],
    technologies: [],
    exhausted_technologies: [],
    relics: [],
    exhausted_relics: [],
    action_cards_count: 2,
    secret_objectives_count: 1,
    held_action_cards: ["direct_hit"],
    held_secret_objectives: ["faa"],
    leaders: {},
  },
  {
    id: "p2",
    faction: "Barony of Letnev",
    victory_points: 2,
    trade_goods: 1,
    commodities: 2,
    tactic_tokens: 2,
    fleet_tokens: 4,
    strategic_tokens: 1,
    passed: false,
    strategy_cards: ["pok2diplomacy"],
    exhausted_strategy_cards: [],
    technologies: [],
    exhausted_technologies: [],
    relics: [],
    exhausted_relics: [],
    action_cards_count: 3,
    secret_objectives_count: 1,
    held_action_cards: [], // Redacted by server
    held_secret_objectives: [],
    leaders: {},
  },
];

describe("PlayerSheet Component & Human Readable Metadata", () => {
  it("resolves cards to accessible click targets for a reusable detail panel", () => {
    const onInspectCard = vi.fn();
    render(<PlayerSheet players={mockPlayers} userSeat="p1" onInspectCard={onInspectCard} />);

    // Strategy cards pok1leadership and pok6warfare mapped to readable names
    const scLeadership = screen.getByTestId("strategy-card-badge-pok1leadership");
    expect(scLeadership).toHaveTextContent("1. Leadership");
    fireEvent.click(scLeadership);
    expect(onInspectCard).toHaveBeenCalledWith({ kind: "strategy", id: "pok1leadership" });

    const scWarfare = screen.getByTestId("strategy-card-badge-pok6warfare");
    expect(scWarfare).toHaveTextContent("6. Warfare");
    expect(scWarfare).toHaveAttribute("type", "button");

    // Secret objective "faa" mapped to Forge an Alliance with description and points
    const soFaa = screen.getByTestId("secret-objective-item-faa");
    expect(soFaa).toHaveTextContent("Forge an Alliance");
    expect(soFaa).toHaveTextContent("Control 4 cultural planets.");
    expect(soFaa).toHaveTextContent("1 VP");
    fireEvent.click(soFaa.querySelector("button")!);
    expect(onInspectCard).toHaveBeenCalledWith({ kind: "secretObjective", id: "faa" });

    // Action card direct_hit mapped to Direct Hit
    const acDirectHit = screen.getByTestId("action-card-item-direct_hit");
    expect(acDirectHit).toHaveTextContent("Direct Hit");
  });

  it("renders private cards exclusively for the viewer seat", () => {
    // Viewer is p1
    render(<PlayerSheet players={mockPlayers} userSeat="p1" />);

    // p1 sees own private cards
    expect(screen.getByTestId("secret-objective-item-faa")).toBeInTheDocument();
    expect(screen.getByTestId("action-card-item-direct_hit")).toBeInTheDocument();

    const privateCards = screen.getAllByTestId("player-card");
    expect(privateCards).toHaveLength(2);

    // Invariant: p2 must not have private-hand-section
    const p2Card = privateCards[1];
    expect(p2Card.querySelector('[data-testid="private-hand-section"]')).toBeNull();
  });

  it("renders zero private cards when viewer is spectator", () => {
    // Viewer is spectator (no seat)
    const { container } = render(<PlayerSheet players={mockPlayers} />);

    // Invariant: no private card elements exist anywhere in DOM
    const privateElements = container.querySelectorAll('[data-private-card="true"]');
    expect(privateElements).toHaveLength(0);

    // Only public counts are rendered
    expect(screen.getAllByText(/Action Cards:/i)).toHaveLength(2);
  });

  it("opens only the owner’s private cards", () => {
    const onInspectCard = vi.fn();
    render(<PlayerSheet players={mockPlayers} userSeat="p1" onInspectCard={onInspectCard} />);
    fireEvent.click(screen.getByTestId("action-card-item-direct_hit").querySelector("button")!);
    expect(onInspectCard).toHaveBeenCalledWith({ kind: "action", id: "direct_hit" });
    expect(screen.queryByTestId("action-card-item-nonexistent")).not.toBeInTheDocument();
    render(<CardDetails subject={{ kind: "strategy", id: "pok1leadership" }} onClose={vi.fn()} />);
    expect(screen.getByTestId("detail-panel")).toHaveTextContent("Gain 3 command tokens");
  });

  it("renders dense additional player stats with icons and tooltips", () => {
    const mockBoard: import("../protocol/types.ts").BoardView = {
      map_tiles: [
        {
          system_id: "1",
          label: "Jord",
          q: 0,
          r: 0,
          planets: [{ id: "jord", label: "Jord", resources: 4, influence: 2 }],
        },
      ],
      systems: {
        "1": {
          system_id: "1",
          command_tokens: [],
          planets: {
            jord: { planet_id: "jord", controlled_by: "p1", exhausted: false },
          },
          units: [
            { owner: "p1", unit_type: "carrier", damaged: false },
            { owner: "p1", unit_type: "space_dock", planet: "jord", damaged: false },
          ],
        },
      },
    };

    render(<PlayerSheet players={mockPlayers} userSeat="p1" board={mockBoard} />);

    // Stats rows rendered for players
    const statsRows = screen.getAllByTestId("player-stats-row");
    expect(statsRows).toHaveLength(2);

    // p1 stats (controls Jord: 4 res, 2 inf; 1 system, 1 planet; dock on Jord: 4 + 2 = 6 production)
    const p1Card = screen.getAllByTestId("player-card")[0];

    const resBadge = p1Card.querySelector('[data-testid="player-resources"]');
    expect(resBadge).toBeInTheDocument();
    expect(resBadge).toHaveTextContent("4/4");
    expect(resBadge).toHaveAttribute("title", "Resources: 4 ready / 4 total");
    expect(resBadge?.querySelector("svg")).toBeInTheDocument();

    const infBadge = p1Card.querySelector('[data-testid="player-influence"]');
    expect(infBadge).toBeInTheDocument();
    expect(infBadge).toHaveTextContent("2/2");
    expect(infBadge).toHaveAttribute("title", "Influence: 2 ready / 2 total");
    expect(infBadge?.querySelector("svg")).toBeInTheDocument();

    const sysBadge = p1Card.querySelector('[data-testid="player-controlled-systems"]');
    expect(sysBadge).toBeInTheDocument();
    expect(sysBadge).toHaveTextContent("1");
    expect(sysBadge).toHaveAttribute("title", "Controlled Systems: 1");
    expect(sysBadge?.querySelector("svg")).toBeInTheDocument();

    const planetBadge = p1Card.querySelector('[data-testid="player-controlled-planets"]');
    expect(planetBadge).toBeInTheDocument();
    expect(planetBadge).toHaveTextContent("1");
    expect(planetBadge).toHaveAttribute("title", "Controlled Planets: 1");
    expect(planetBadge?.querySelector("svg")).toBeInTheDocument();

    const prodBadge = p1Card.querySelector('[data-testid="player-production-capacity"]');
    expect(prodBadge).toBeInTheDocument();
    expect(prodBadge).toHaveTextContent("6/6");
    expect(prodBadge).toHaveAttribute("title", "Production Capacity: 6 available / 6 total");
    expect(prodBadge?.querySelector("svg")).toBeInTheDocument();
  });

  it("shows 0 available production capacity when system has player command token", () => {
    const mockBoard: import("../protocol/types.ts").BoardView = {
      map_tiles: [
        {
          system_id: "1",
          label: "Jord",
          q: 0,
          r: 0,
          planets: [{ id: "jord", label: "Jord", resources: 4, influence: 2 }],
        },
      ],
      systems: {
        "1": {
          system_id: "1",
          command_tokens: ["p1"], // Activated by p1
          planets: {
            jord: { planet_id: "jord", controlled_by: "p1", exhausted: false },
          },
          units: [{ owner: "p1", unit_type: "space_dock", planet: "jord", damaged: false }],
        },
      },
    };

    render(<PlayerSheet players={mockPlayers} userSeat="p1" board={mockBoard} />);
    const p1Card = screen.getAllByTestId("player-card")[0];
    const prodBadge = p1Card.querySelector('[data-testid="player-production-capacity"]');
    expect(prodBadge).toBeInTheDocument();
    expect(prodBadge).toHaveTextContent("0/6");
    expect(prodBadge).toHaveAttribute("title", "Production Capacity: 0 available / 6 total");
  });
});
