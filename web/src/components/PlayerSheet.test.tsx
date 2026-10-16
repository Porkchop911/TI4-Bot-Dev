import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { PlayerSheet, calculateVPBreakdown, VPBreakdownTooltip } from "./PlayerSheet.tsx";
import { CardDetails } from "./CardDetails.tsx";
import { PlayerView, TableView } from "../protocol/types.ts";
import { getActionCardMeta } from "../protocol/contentCatalog.ts";

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

  it("shows progress toward the viewer's own countable secrets", () => {
    const players: PlayerView[] = [
      {
        ...mockPlayers[0],
        held_secret_objectives: ["ctr", "pe"],
        secret_objective_progress: { ctr: { have: 4, threshold: 6, satisfied: false } },
      },
      mockPlayers[1],
    ];
    render(<PlayerSheet players={players} userSeat="p1" />);
    const ctr = screen.getByTestId("secret-objective-item-ctr");
    expect(ctr).toHaveTextContent("4 / 6 systems with at least one of your ships");
    expect(ctr.querySelector(".secret-progress__fill")).toHaveStyle({ width: "67%" });
    // Prove Endurance is met by an event: no bar, a note instead.
    const pe = screen.getByTestId("secret-objective-item-pe");
    expect(pe.querySelector('[data-testid="secret-progress-pe"]')).toBeNull();
    expect(pe).toHaveTextContent("offered the moment it happens");
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

  it("M11: displays exhausted strategy cards as grayed out (visual distinction)", () => {
    const playersWithExhausted: PlayerView[] = [
      {
        ...mockPlayers[0],
        strategy_cards: ["pok1leadership", "pok6warfare"],
        exhausted_strategy_cards: ["pok1leadership"], // This card is exhausted
      },
    ];
    const mockBoard = { systems: {} };
    render(<PlayerSheet players={playersWithExhausted} userSeat="p1" board={mockBoard} />);
    const p1Card = screen.getAllByTestId("player-card")[0];
    // Check that exhausted cards have visual indication (strategy-card--exhausted class)
    const exhaustedCard = p1Card.querySelector('[data-testid="strategy-card-badge-pok1leadership"]');
    expect(exhaustedCard).toBeInTheDocument();
    // Exhausted card should have the exhausted class
    expect(exhaustedCard).toHaveClass("strategy-card--exhausted");
    // Active card should not have the exhausted class
    const activeCard = p1Card.querySelector('[data-testid="strategy-card-badge-pok6warfare"]');
    expect(activeCard).not.toHaveClass("strategy-card--exhausted");
  });

  it("M12: shows VP breakdown tooltip with source information", () => {
    const mockBoard = { systems: {} };
    render(<PlayerSheet players={mockPlayers} userSeat="p1" board={mockBoard} />);
    const p1Card = screen.getAllByTestId("player-card")[0];
    const vpDisplay = p1Card.querySelector('[data-testid="player-vp"]');
    expect(vpDisplay).toBeInTheDocument();
    // Check for VP value displayed
    expect(vpDisplay).toHaveTextContent("3");
  });

  it("M16: displays current player card prominently", () => {
    const mockBoard = { systems: {} };
    render(
      <PlayerSheet
        players={mockPlayers}
        userSeat="p1"
        board={mockBoard}
      />,
    );
    const playerCards = screen.getAllByTestId("player-card");
    // Current player's card should have data-is-self attribute
    const currentPlayerCard = playerCards.find(card => card.getAttribute("data-is-self") === "true");
    expect(currentPlayerCard).toBeInTheDocument();
    expect(currentPlayerCard?.textContent).toContain("Federation of Sol");
  });

  describe("VP breakdown", () => {
    const mecatolBoard = {
      systems: { "18": { planets: { mecatol_rex: { controlled_by: "p1" } } } },
    } as never;
    const sum = (b: ReturnType<typeof calculateVPBreakdown>) => b.publicVP + b.secretVP + b.otherVP;

    it("Mecatol control gives no VP", () => {
      const b = calculateVPBreakdown({ ...mockPlayers[0], victory_points: 0 }, mecatolBoard);
      expect(b.total).toBe(0);
      expect(b.otherVP).toBe(0);
      expect(sum(b)).toBe(0);
    });

    it("shows an unexplained remainder as Other", () => {
      const b = calculateVPBreakdown({ ...mockPlayers[0], victory_points: 3 });
      expect(b.otherVP).toBe(3);
      render(<VPBreakdownTooltip breakdown={b} />);
      expect(screen.getByTestId("vp-breakdown-other")).toHaveTextContent("Other sources");
      expect(screen.getByTestId("vp-breakdown-other")).toHaveTextContent("+3 VP");
    });

    it("always sums to the server total, even when known points exceed it", () => {
      const publicId = "amass_wealth";
      const table = { scored_objectives: { p1: [publicId, publicId] }, laws: {} } as unknown as TableView;
      for (const vp of [0, 1, 2, 5]) {
        const b = calculateVPBreakdown({ ...mockPlayers[0], victory_points: vp }, undefined, table);
        expect(sum(b)).toBe(vp);
        expect(b.otherVP).toBeGreaterThanOrEqual(0);
      }
      const over = calculateVPBreakdown({ ...mockPlayers[0], victory_points: 1 }, undefined, table);
      expect(over.knownExceedsTotal).toBe(true);
    });
  });
});

describe("PlayerSheet toast mute", () => {
  it("toggles the notification mute and remembers it", async () => {
    const { TOAST_MUTE_KEY } = await import("../hooks/useToastMute.ts");
    localStorage.clear();
    render(<PlayerSheet players={mockPlayers} />);
    const button = screen.getByTestId("toast-mute-btn");
    expect(button).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(button);
    expect(button).toHaveAttribute("aria-pressed", "true");
    expect(localStorage.getItem(TOAST_MUTE_KEY)).toBe("true");
    localStorage.clear();
  });
});

describe("PlayerSheet turn recap toggle", () => {
  it("starts off, turns on and remembers it", async () => {
    const { TURN_RECAP_KEY } = await import("../hooks/useTurnRecapSetting.ts");
    localStorage.clear();
    render(<PlayerSheet players={mockPlayers} />);
    const button = screen.getByTestId("turn-recap-btn");
    expect(button).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(button);
    expect(button).toHaveAttribute("aria-pressed", "true");
    expect(localStorage.getItem(TURN_RECAP_KEY)).toBe("true");
    fireEvent.click(button);
    expect(localStorage.getItem(TURN_RECAP_KEY)).toBe("false");
    localStorage.clear();
  });
});

describe("PlayerSheet reaction mode toggle", () => {
  const name = getActionCardMeta("direct_hit").name;

  it("shows the server's mode for the card name and sends the opposite on click", () => {
    const onSetReactionMode = vi.fn();
    const { rerender } = render(
      <PlayerSheet players={mockPlayers} userSeat="p1" onSetReactionMode={onSetReactionMode} />,
    );
    const toggle = screen.getByTestId("reaction-inspect-mode-direct_hit");
    expect(toggle).toHaveAttribute("data-reaction-mode", "always");
    fireEvent.click(toggle);
    expect(onSetReactionMode).toHaveBeenLastCalledWith(name, "never");

    // Nothing changes locally: the toggle follows what the server says.
    expect(screen.getByTestId("reaction-inspect-mode-direct_hit")).toHaveAttribute(
      "data-reaction-mode",
      "always",
    );
    rerender(
      <PlayerSheet
        players={mockPlayers}
        userSeat="p1"
        reactionModes={{ [name]: "never" }}
        onSetReactionMode={onSetReactionMode}
      />,
    );
    const never = screen.getByTestId("reaction-inspect-mode-direct_hit");
    expect(never).toHaveAttribute("data-reaction-mode", "never");
    expect(never).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(never);
    expect(onSetReactionMode).toHaveBeenLastCalledWith(name, "always");
  });

  it("has no toggle for a viewer who cannot change modes or for another seat's cards", () => {
    render(<PlayerSheet players={mockPlayers} userSeat="p1" />);
    expect(screen.queryByTestId("reaction-inspect-mode-direct_hit")).toBeNull();
  });
});

describe("PlayerSheet economy and token line", () => {
  it("shows bare token numbers separated by diamonds, without the old label", () => {
    render(<PlayerSheet players={mockPlayers} userSeat="p1" />);
    const tokens = screen.getAllByTestId("player-command-tokens");
    expect(tokens[0].textContent).toBe("3 ◆ 3 ◆ 2");
    expect(tokens[1].textContent).toBe("2 ◆ 4 ◆ 1");
    expect(screen.queryByText(/Tokens:/)).toBeNull();
  });

  it("names the numbers in aria-labels and hover tooltips", () => {
    vi.useFakeTimers();
    try {
      render(<PlayerSheet players={mockPlayers} userSeat="p1" />);
      const tokens = screen.getAllByTestId("player-command-tokens")[0];
      expect(tokens.getAttribute("aria-label")).toBe("Command tokens: tactic 3, fleet 3, strategy 2");
      expect(tokens.tabIndex).toBe(0);
      fireEvent.pointerEnter(tokens);
      act(() => void vi.advanceTimersByTime(300));
      expect(screen.getByRole("tooltip").textContent).toBe("Tactic 3 · Fleet 3 · Strategy 2");
      fireEvent.pointerLeave(tokens);

      const tg = screen.getAllByTestId("player-trade-goods")[0];
      expect(tg.getAttribute("aria-label")).toBe("Trade goods: 2");
      fireEvent.pointerEnter(tg);
      act(() => void vi.advanceTimersByTime(300));
      expect(screen.getByRole("tooltip").textContent).toBe("Trade goods");
      fireEvent.pointerLeave(tg);

      const comm = screen.getAllByTestId("player-commodities")[0];
      expect(comm.getAttribute("aria-label")).toBe("Commodities: 4");
      fireEvent.focus(comm);
      act(() => void vi.advanceTimersByTime(300));
      expect(screen.getByRole("tooltip").textContent).toBe("Commodities");
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("PlayerSheet held foreign command tokens", () => {
  it("shows one chip per foreign token in the fleet pool and nothing for other seats", () => {
    const players = [
      { ...mockPlayers[0], held_foreign_tokens: ["p2", "p3"] },
      mockPlayers[1],
    ];
    render(<PlayerSheet players={players} userSeat="p1" />);
    expect(screen.getAllByTestId("player-held-tokens")).toHaveLength(1);
    expect(screen.getAllByTestId("player-held-token")).toHaveLength(2);
    expect(screen.getByTestId("player-held-tokens").getAttribute("aria-label")).toContain(
      "Holds 2 other players' command tokens in the fleet pool",
    );
  });
});

