import React from "react";
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/react";
import { TechnologyModal } from "./TechnologyModal.tsx";
import { PlayerIdentityProvider } from "../presentation/PlayerIdentity.tsx";
import type { LobbyDto, PlayerView, BoardView, PendingChoiceDto } from "../protocol/types.ts";
import { partialContext } from "../test/wire.ts";

const mockLobby: LobbyDto = {
  game_id: "test-game",
  phase: "running",
  lobby_version: 1,
  host_player_id: "player1",
  slots: [
    {
      slot_id: "s1",
      position: 1,
      occupant: "player1",
      nickname: "Alice",
      connected: true,
      can_take_over: false,
    },
    {
      slot_id: "s2",
      position: 2,
      occupant: "player2",
      nickname: "Bob",
      connected: true,
      can_take_over: false,
    },
  ],
};

const mockPlayers: PlayerView[] = [
  {
    id: "player1",
    faction: "sol",
    victory_points: 3,
    trade_goods: 2,
    commodities: 4,
    tactic_tokens: 3,
    fleet_tokens: 3,
    strategic_tokens: 2,
    passed: false,
    strategy_cards: [],
    exhausted_strategy_cards: [],
    technologies: ["amd", "gd", "cr2", "x89c4"],
    exhausted_technologies: ["gd"],
    relics: [],
    exhausted_relics: [],
    action_cards_count: 2,
    secret_objectives_count: 1,
    leaders: {},
  },
  {
    id: "player2",
    faction: "hacan",
    victory_points: 2,
    trade_goods: 5,
    commodities: 6,
    tactic_tokens: 2,
    fleet_tokens: 2,
    strategic_tokens: 3,
    passed: false,
    strategy_cards: [],
    exhausted_strategy_cards: [],
    technologies: ["st", "cr2", "dn2"],
    exhausted_technologies: [],
    relics: [],
    exhausted_relics: [],
    action_cards_count: 3,
    secret_objectives_count: 1,
    leaders: {},
  },
];

function renderWithIdentity(ui: React.ReactElement) {
  return render(
    <PlayerIdentityProvider lobby={mockLobby} seatingOrder={["player1", "player2"]}>
      {ui}
    </PlayerIdentityProvider>,
  );
}

describe("TechnologyModal", () => {
  it("does not render when isOpen is false", () => {
    renderWithIdentity(<TechnologyModal isOpen={false} onClose={vi.fn()} players={mockPlayers} />);
    expect(screen.queryByTestId("technology-modal")).not.toBeInTheDocument();
  });

  it("labels each standard technology card with its track (shown on phones, where the header row is hidden)", () => {
    renderWithIdentity(<TechnologyModal isOpen={true} onClose={vi.fn()} players={mockPlayers} />);
    expect(screen.getByTestId("tech-card-amd")).toHaveAttribute("data-track", "Propulsion");
    expect(screen.getByTestId("tech-card-nm")).toHaveAttribute("data-track", "Biotic");
  });

  it("renders modal with header, roster, and close button when isOpen is true", () => {
    const onClose = vi.fn();
    renderWithIdentity(<TechnologyModal isOpen={true} onClose={onClose} players={mockPlayers} />);

    expect(screen.getByTestId("technology-modal")).toBeInTheDocument();
    expect(screen.getByText("Technologies")).toBeInTheDocument();
    expect(screen.getByTestId("technology-modal-close")).toBeInTheDocument();

    // Roster legend
    expect(screen.getByTestId("tech-modal-roster")).toBeInTheDocument();
    expect(screen.getByTestId("tech-roster-player-player1")).toHaveTextContent("Alice");
    expect(screen.getByTestId("tech-roster-player-player2")).toHaveTextContent("Bob");

    // Close button triggers onClose
    fireEvent.click(screen.getByTestId("technology-modal-close"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("renders all four color tracks and unit upgrades", () => {
    renderWithIdentity(<TechnologyModal isOpen={true} onClose={vi.fn()} players={mockPlayers} />);

    // Track columns
    expect(screen.getByTestId("tech-track-propulsion")).toBeInTheDocument();
    expect(screen.getByTestId("tech-track-biotic")).toBeInTheDocument();
    expect(screen.getByTestId("tech-track-cybernetic")).toBeInTheDocument();
    expect(screen.getByTestId("tech-track-warfare")).toBeInTheDocument();
    expect(screen.getByTestId("tech-track-unitupgrade")).toBeInTheDocument();

    // Key technologies in tracks
    expect(screen.getByTestId("tech-card-amd")).toBeInTheDocument();
    expect(screen.getByTestId("tech-name-amd")).toHaveTextContent("Antimass Deflectors");

    expect(screen.getByTestId("tech-card-nm")).toBeInTheDocument();
    expect(screen.getByTestId("tech-name-nm")).toHaveTextContent("Neural Motivator");

    expect(screen.getByTestId("tech-card-st")).toBeInTheDocument();
    expect(screen.getByTestId("tech-name-st")).toHaveTextContent("Sarween Tools");

    expect(screen.getByTestId("tech-card-ps")).toBeInTheDocument();
    expect(screen.getByTestId("tech-name-ps")).toHaveTextContent("Plasma Scoring");

    // Unit upgrades
    expect(screen.getByTestId("tech-card-ws")).toBeInTheDocument();
    expect(screen.getByTestId("tech-name-ws")).toHaveTextContent("War Sun");
    expect(screen.getByTestId("tech-card-cr2")).toBeInTheDocument();
    expect(screen.getByTestId("tech-name-cr2")).toHaveTextContent("Cruiser II");
    expect(screen.getByTestId("tech-card-dn2")).toBeInTheDocument();
    expect(screen.getByTestId("tech-name-dn2")).toHaveTextContent("Dreadnought II");
  });

  it("displays effect descriptions and prerequisite badges", () => {
    renderWithIdentity(<TechnologyModal isOpen={true} onClose={vi.fn()} players={mockPlayers} />);

    // Antimass Deflectors description (no prereqs: "—")
    const amdEffect = screen.getByTestId("tech-effect-amd");
    expect(amdEffect.textContent).toContain("Your ships can move into and through asteroid fields");
    expect(screen.getByTestId("tech-prereqs-amd")).toHaveTextContent("—");

    // Gravity Drive (1 Blue prereq)
    const gdPrereqs = screen.getByTestId("tech-prereqs-gd");
    expect(gdPrereqs.textContent).toContain("B");

    // Cruiser II (GYR prereqs)
    const cr2Prereqs = screen.getByTestId("tech-prereqs-cr2");
    expect(cr2Prereqs.textContent).toContain("G");
    expect(cr2Prereqs.textContent).toContain("Y");
    expect(cr2Prereqs.textContent).toContain("R");
  });

  it("renders every technology card's own text (never an empty effect), and a CSS-independent clamp hook", () => {
    renderWithIdentity(<TechnologyModal isOpen={true} onClose={vi.fn()} players={mockPlayers} />);

    const effects = screen.getAllByTestId(/^tech-effect-(?!more-)/);
    expect(effects.length).toBeGreaterThan(20);
    for (const el of effects) {
      expect(el.textContent?.trim().length, el.getAttribute("data-testid") ?? "").toBeGreaterThan(5);
      expect(el).toHaveAttribute("data-expanded", "false");
    }
    // jsdom has no layout, so nothing counts as clamped: no More button on a card.
    expect(screen.queryByTestId("tech-effect-more-amd")).toBeNull();
  });

  it("displays player markers with symbol, position/nickname, and seat color for researched tech", () => {
    renderWithIdentity(<TechnologyModal isOpen={true} onClose={vi.fn()} players={mockPlayers} />);

    // Player 1 researched AMD
    const amdP1Marker = screen.getByTestId("player-tech-marker-player1-amd");
    expect(amdP1Marker).toBeInTheDocument();
    // Position 1 color in SEAT_COLORS is #3B82F6, symbol is ●
    expect(amdP1Marker.textContent).toBe("●");
    expect(amdP1Marker).toHaveAttribute("title", "Alice (sol)");

    // Player 2 did NOT research AMD
    expect(screen.queryByTestId("player-tech-marker-player2-amd")).not.toBeInTheDocument();

    // Both Player 1 and Player 2 researched Cruiser II
    expect(screen.getByTestId("player-tech-marker-player1-cr2")).toBeInTheDocument();
    expect(screen.getByTestId("player-tech-marker-player2-cr2")).toBeInTheDocument();
  });

  it("indicates exhausted status for exhausted technologies", () => {
    renderWithIdentity(<TechnologyModal isOpen={true} onClose={vi.fn()} players={mockPlayers} />);

    // Player 1 researched GD and exhausted it
    const gdMarker = screen.getByTestId("player-tech-marker-player1-gd");
    expect(gdMarker).toHaveAttribute("data-exhausted", "true");
    expect(gdMarker).toHaveAttribute("title", "Alice (sol) - Exhausted");

    // AMD is not exhausted
    const amdMarker = screen.getByTestId("player-tech-marker-player1-amd");
    expect(amdMarker).toHaveAttribute("data-exhausted", "false");
    expect(amdMarker).toHaveAttribute("title", "Alice (sol)");
  });

  it("handles alias matching (e.g. x89 variant)", () => {
    renderWithIdentity(<TechnologyModal isOpen={true} onClose={vi.fn()} players={mockPlayers} />);

    // Player 1 has x89c4, which matches x89c4
    expect(screen.getByTestId("player-tech-marker-player1-x89c4")).toBeInTheDocument();
  });

  it("supports future research selection phase with selectable cards and confirm button", () => {
    const onSelectTech = vi.fn();
    const onConfirmResearch = vi.fn();
    const researchPlayers: PlayerView[] = mockPlayers.map((p) =>
      p.id === "player1" ? { ...p, technologies: ["amd"] } : p,
    );

    const { rerender } = renderWithIdentity(
      <TechnologyModal
        isOpen={true}
        onClose={vi.fn()}
        players={researchPlayers}
        viewerSeat="player1"
        selectableTechIds={["gd", "sr"]}
        selectedTechId={null}
        onSelectTech={onSelectTech}
        onConfirmResearch={onConfirmResearch}
        isResearchMode={true}
      />,
    );

    // Research mode banner
    expect(screen.getByText("Select Research")).toBeInTheDocument();

    // Selectable cards have data-selectable="true"
    const gdCard = screen.getByTestId("tech-card-gd");
    expect(gdCard).toHaveAttribute("data-selectable", "true");

    const amdCard = screen.getByTestId("tech-card-amd");
    expect(amdCard).toHaveAttribute("data-selectable", "false");

    // Confirm button is disabled when nothing selected
    const confirmBtn = screen.getByTestId("confirm-research-btn");
    expect(confirmBtn).toBeDisabled();

    // Click selectable card
    fireEvent.click(gdCard);
    expect(onSelectTech).toHaveBeenCalledWith("gd");

    // Rerender with selectedTechId="gd"
    rerender(
      <PlayerIdentityProvider lobby={mockLobby} seatingOrder={["player1", "player2"]}>
        <TechnologyModal
          isOpen={true}
          onClose={vi.fn()}
          players={researchPlayers}
          viewerSeat="player1"
          selectableTechIds={["gd", "sr"]}
          selectedTechId="gd"
          onSelectTech={onSelectTech}
          onConfirmResearch={onConfirmResearch}
          isResearchMode={true}
        />
      </PlayerIdentityProvider>,
    );

    expect(screen.getByTestId("tech-card-gd")).toHaveAttribute("data-selected", "true");
    expect(screen.getByTestId("confirm-research-btn")).not.toBeDisabled();

    fireEvent.click(screen.getByTestId("confirm-research-btn"));
    expect(onConfirmResearch).toHaveBeenCalledWith("gd");
  });

  it("shows the Enigmatic Device question as a research: prerequisites apply and the pick is submitted", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const researchPlayers: PlayerView[] = mockPlayers.map((p) =>
      p.id === "player1" ? { ...p, technologies: ["amd"] } : p,
    );
    const choice: PendingChoiceDto = {
      nonce: "n1",
      actor: "player1",
      prompt: "gain which technology",
      context: partialContext({
        subtype: "enigmaticdevice_choose_technology",
        source: { Content: "enigmaticdevice" },
      }),
      options: ["gd", "sr", "amd"].map((id) => ({
        id,
        label: `gain ${id}`,
        kind: "technology",
        payload: {},
      })),
    };
    renderWithIdentity(
      <TechnologyModal
        isOpen={true}
        onClose={vi.fn()}
        players={researchPlayers}
        viewerSeat="player1"
        choice={choice}
        onSubmit={onSubmit}
      />,
    );
    expect(screen.getByText("Select Research")).toBeInTheDocument();
    // Offered by the engine and without prerequisites: pickable. Held or unlisted: not.
    expect(screen.getByTestId("tech-card-sr")).toHaveAttribute("data-selectable", "true");
    expect(screen.getByTestId("tech-card-amd")).toHaveAttribute("data-selectable", "false");
    expect(screen.getByTestId("tech-card-ps")).toHaveAttribute("data-selectable", "false");
    fireEvent.click(screen.getByTestId("tech-card-sr"));
    fireEvent.click(screen.getByTestId("confirm-research-btn"));
    await vi.waitFor(() => expect(onSubmit).toHaveBeenCalledWith("sr"));
  });

  it("renders faction technologies first at the top of modal body for Sol", () => {
    const researchPlayers: PlayerView[] = [
      {
        ...mockPlayers[0],
        faction: "sol",
        technologies: [],
      },
      mockPlayers[1],
    ];

    renderWithIdentity(
      <TechnologyModal
        isOpen={true}
        onClose={vi.fn()}
        players={researchPlayers}
        viewerSeat="player1"
        isResearchMode={true}
      />,
    );

    const factionSection = screen.getByTestId("faction-technologies-section");
    expect(factionSection).toBeInTheDocument();
    expect(factionSection).toHaveTextContent(/Faction Technologies/i);
    expect(factionSection).toHaveTextContent(/Sol/i);

    // Sol faction techs: Spec Ops II and Advanced Carrier II
    expect(screen.getByTestId("tech-card-so2")).toBeInTheDocument();
    expect(screen.getByTestId("tech-card-ac2")).toBeInTheDocument();

    // Verify faction section appears before standard track headers in DOM order
    const propulsionHeader = screen.getByTestId("tech-track-propulsion");
    expect(
      factionSection.compareDocumentPosition(propulsionHeader) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("shows the viewing seat's own faction technologies, not player 1's", () => {
    renderWithIdentity(
      <TechnologyModal isOpen={true} onClose={vi.fn()} players={mockPlayers} viewerSeat="player2" />,
    );
    const section = screen.getByTestId("faction-technologies-section");
    expect(section).toHaveTextContent(/Hacan/i);
    expect(screen.queryByTestId("tech-card-so2")).not.toBeInTheDocument();
    expect(screen.queryByTestId("tech-card-ac2")).not.toBeInTheDocument();
  });

  it("shows no faction technologies section for a spectator (no seat)", () => {
    renderWithIdentity(<TechnologyModal isOpen={true} onClose={vi.fn()} players={mockPlayers} />);
    expect(screen.queryByTestId("faction-technologies-section")).not.toBeInTheDocument();
    expect(screen.queryByTestId("tech-card-so2")).not.toBeInTheDocument();
  });

  it("shows tech skips, handles exhausted vs ready, and toggling unlocks higher tier techs", () => {
    const researchPlayers: PlayerView[] = [
      {
        ...mockPlayers[0],
        faction: "sol",
        technologies: [], // No technologies owned
      },
      mockPlayers[1],
    ];

    const mockBoard = {
      systems: {
        "1": {
          system_id: "1",
          command_tokens: [],
          units: [],
          planets: {
            wellon: {
              planet_id: "wellon",
              controlled_by: "player1",
              exhausted: false,
            },
            lazar: {
              planet_id: "lazar",
              controlled_by: "player1",
              exhausted: true,
            },
          },
        },
      },
    } as unknown as BoardView;

    renderWithIdentity(
      <TechnologyModal
        isOpen={true}
        onClose={vi.fn()}
        players={researchPlayers}
        board={mockBoard}
        viewerSeat="player1"
        isResearchMode={true}
      />,
    );

    // Ready skip: Wellon
    const wellonPill = screen.getByTestId("tech-skip-wellon");
    expect(wellonPill).toBeInTheDocument();
    expect(wellonPill).not.toBeDisabled();
    expect(wellonPill).toHaveTextContent("Ready");

    // Exhausted skip: Lazar
    const lazarPill = screen.getByTestId("tech-skip-lazar");
    expect(lazarPill).toBeInTheDocument();
    expect(lazarPill).toBeDisabled();
    expect(lazarPill).toHaveTextContent("(Exhausted)");

    // Graviton Laser System (Cybernetic Tier 1) requires 1 yellow prereq
    // Initially, player owns 0 yellow and skip is not toggled -> unresearchable!
    const gravitonCard = screen.getByTestId("tech-card-gls");
    expect(gravitonCard).toHaveAttribute("data-researchable", "false");

    // Toggle Wellon skip ON
    fireEvent.click(wellonPill);
    expect(wellonPill).toHaveTextContent("Active (+1)");

    // Graviton now satisfied with +1 Cybernetic skip -> researchable!
    expect(gravitonCard).toHaveAttribute("data-researchable", "true");

    // Untoggle Wellon skip OFF
    fireEvent.click(wellonPill);
    expect(wellonPill).toHaveTextContent("Ready");
    expect(gravitonCard).toHaveAttribute("data-researchable", "false");
  });

  it("handles Option B multi-tech research for Technology Primary: Free for 1, 6 resources for 2", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const researchPlayers: PlayerView[] = [
      {
        ...mockPlayers[0],
        faction: "sol",
        trade_goods: 8,
        technologies: [],
      },
      mockPlayers[1],
    ];

    const choice: PendingChoiceDto = {
      actor: "player1",
      nonce: "dec-tech-primary",
      prompt: "Research a technology",
      context: partialContext({
        subtype: "research_technology",
        source: {
          StrategyCard: {
            card: "Technology",
            secondary: false,
          },
        },
      }),
      options: [
        { id: "amd", label: "Antimass Deflectors", kind: "research", payload: {} },
        { id: "ps", label: "Plasma Scoring", kind: "research", payload: {} },
        { id: "decline", label: "Decline", kind: "decline", payload: {} },
      ],
    };

    renderWithIdentity(
      <TechnologyModal
        isOpen={true}
        onClose={vi.fn()}
        players={researchPlayers}
        viewerSeat="player1"
        choice={choice}
        onSubmit={onSubmit}
        isResearchMode={true}
      />,
    );

    // Initial state: 0 / 2 selected, Free cost
    expect(screen.getByText(/Selected:/)).toHaveTextContent("0 / 2");
    expect(screen.getByText("Cost: Free")).toBeInTheDocument();
    const confirmBtn = screen.getByTestId("confirm-research-btn");
    expect(confirmBtn).toBeDisabled();

    // Select 1 tech: amd
    const amdCard = screen.getByTestId("tech-card-amd");
    fireEvent.click(amdCard);
    expect(screen.getByText(/Selected:/)).toHaveTextContent("1 / 2");
    expect(screen.getByText("Cost: Free")).toBeInTheDocument();
    expect(confirmBtn).not.toBeDisabled();
    expect(confirmBtn).toHaveTextContent("Confirm Research (1 Tech - Free)");

    // Select 2nd tech: ps
    const psCard = screen.getByTestId("tech-card-ps");
    fireEvent.click(psCard);
    expect(screen.getByText(/Selected:/)).toHaveTextContent("2 / 2");
    expect(screen.getAllByLabelText("6 resources").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText(/Cost:/)).toBeInTheDocument();
    expect(confirmBtn).toHaveTextContent("Confirm Research (2 Techs - 6)");
    expect(within(confirmBtn).getByLabelText("6 resources")).toBeInTheDocument();

    // Click confirm -> executes Option B two-step research pipeline
    fireEvent.click(confirmBtn);
    expect(onSubmit).toHaveBeenCalledWith("amd");
  });

  it("updates prerequisites to include the 1st selected tech when researching two techs", () => {
    const researchPlayers: PlayerView[] = [
      {
        ...mockPlayers[0],
        faction: "sol",
        trade_goods: 8,
        technologies: [], // 0 technologies owned
      },
      mockPlayers[1],
    ];

    const choice: PendingChoiceDto = {
      actor: "player1",
      nonce: "dec-tech-primary-chain",
      prompt: "Research a technology",
      context: partialContext({
        subtype: "research_technology",
        source: {
          StrategyCard: {
            card: "Technology",
            secondary: false,
          },
        },
      }),
      options: [
        { id: "st", label: "Sarween Tools", kind: "research", payload: {} },
        { id: "ps", label: "Plasma Scoring", kind: "research", payload: {} },
        { id: "decline", label: "Decline", kind: "decline", payload: {} },
      ],
    };

    renderWithIdentity(
      <TechnologyModal
        isOpen={true}
        onClose={vi.fn()}
        players={researchPlayers}
        viewerSeat="player1"
        choice={choice}
        isResearchMode={true}
      />,
    );

    // Graviton Laser System (gls) requires 1 Cybernetic prereq.
    // Initially, player has 0 Cybernetic techs and 0 skips -> unresearchable!
    const glsCard = screen.getByTestId("tech-card-gls");
    expect(glsCard).toHaveAttribute("data-researchable", "false");
    expect(screen.queryByTestId("first-tech-prereq-bonus")).not.toBeInTheDocument();

    // Select 1st tech: Sarween Tools (st, Cybernetic)
    const stCard = screen.getByTestId("tech-card-st");
    fireEvent.click(stCard);
    expect(stCard).toHaveAttribute("data-selected", "true");

    // Skips list now shows the 1st tech prerequisite bonus pill!
    const bonusPill = screen.getByTestId("first-tech-prereq-bonus");
    expect(bonusPill).toBeInTheDocument();
    expect(bonusPill).toHaveTextContent("Sarween Tools");
    expect(bonusPill).toHaveTextContent("Tech 1 (+1)");

    // Graviton Laser System now has its 1 Cybernetic prerequisite met by Sarween Tools -> researchable!
    expect(glsCard).toHaveAttribute("data-researchable", "true");

    // Select Graviton Laser System as 2nd tech
    fireEvent.click(glsCard);
    expect(glsCard).toHaveAttribute("data-selected", "true");
    expect(screen.getByText(/Selected:/)).toHaveTextContent("2 / 2");

    // If user unclicks the 1st tech (Sarween Tools), Graviton cannot stand alone without prereqs, so selection clears
    fireEvent.click(stCard);
    expect(stCard).toHaveAttribute("data-selected", "false");
    expect(glsCard).toHaveAttribute("data-selected", "false");
    expect(glsCard).toHaveAttribute("data-researchable", "false");
    expect(screen.queryByTestId("first-tech-prereq-bonus")).not.toBeInTheDocument();
  });

  describe("Technology primary: the second technology costs 6 resources", () => {
    const primaryChoice = (ids: string[]): PendingChoiceDto => ({
      actor: "player1",
      nonce: "dec-tech-primary-afford",
      prompt: "Research a technology",
      context: partialContext({
        subtype: "research_technology",
        source: { StrategyCard: { card: "Technology", secondary: false } },
      }),
      options: ids.map((id) => ({ id, label: id, kind: "research", payload: {} })),
    });
    const renderPrimary = (tradeGoods: number, owned: string[], ids: string[]) =>
      renderWithIdentity(
        <TechnologyModal
          isOpen={true}
          onClose={vi.fn()}
          players={[
            { ...mockPlayers[0], faction: "sol", trade_goods: tradeGoods, technologies: owned },
            mockPlayers[1],
          ]}
          viewerSeat="player1"
          choice={primaryChoice(ids)}
          onSubmit={vi.fn().mockResolvedValue(undefined)}
          isResearchMode={true}
        />,
      );

    it("with 4 resources no other card can be the second pick, and Confirm stays enabled for the single pick", () => {
      renderPrimary(4, [], ["amd", "ps", "nm"]);
      fireEvent.click(screen.getByTestId("tech-card-amd"));
      expect(screen.getByTestId("tech-card-amd")).toHaveAttribute("data-selected", "true");
      for (const id of ["ps", "nm", "st"]) {
        const card = screen.getByTestId(`tech-card-${id}`);
        expect(card).toHaveAttribute("data-selectable", "false");
        expect(card).toHaveAttribute(
          "data-blocked-reason",
          "Second technology costs 6 resources, you have 4",
        );
      }
      // Clicking a blocked card changes nothing.
      fireEvent.click(screen.getByTestId("tech-card-ps"));
      expect(screen.getByText(/Selected:/)).toHaveTextContent("1 / 2");
      expect(screen.getByTestId("tech-second-pick-hint")).toHaveTextContent(
        "A second technology costs 6 (you have 4). Pick one technology or gain resources.",
      );
      expect(screen.getByTestId("confirm-research-btn")).not.toBeDisabled();
      expect(screen.queryByTestId("tech-clear-second")).not.toBeInTheDocument();
    });

    it("with 6 resources two picks are allowed, no hint shows, and the second pick can be cleared", () => {
      renderPrimary(6, [], ["amd", "ps", "nm"]);
      fireEvent.click(screen.getByTestId("tech-card-amd"));
      expect(screen.getByTestId("tech-card-ps")).toHaveAttribute("data-selectable", "true");
      expect(screen.queryByTestId("tech-second-pick-hint")).not.toBeInTheDocument();
      fireEvent.click(screen.getByTestId("tech-card-ps"));
      expect(screen.getByText(/Selected:/)).toHaveTextContent("2 / 2");
      expect(screen.getByTestId("confirm-research-btn")).not.toBeDisabled();
      fireEvent.click(screen.getByTestId("tech-clear-second"));
      expect(screen.getByText(/Selected:/)).toHaveTextContent("1 / 2");
      expect(screen.getByTestId("tech-card-amd")).toHaveAttribute("data-selected", "true");
      // Clicking the selected second card also takes it back.
      fireEvent.click(screen.getByTestId("tech-card-ps"));
      fireEvent.click(screen.getByTestId("tech-card-ps"));
      expect(screen.getByText(/Selected:/)).toHaveTextContent("1 / 2");
    });

    it("Hyper Metabolism becomes a valid second pick through the first pick's track, though it is not offered", () => {
      renderPrimary(6, ["nm"], ["pa", "amd"]);
      const hm = screen.getByTestId("tech-card-hm");
      expect(hm).toHaveAttribute("data-selectable", "false");
      fireEvent.click(screen.getByTestId("tech-card-pa"));
      expect(hm).toHaveAttribute("data-selectable", "true");
      fireEvent.click(hm);
      expect(hm).toHaveAttribute("data-selected", "true");
    });

    it("a technology left out of the engine's list that was already valid is not a second pick", () => {
      renderPrimary(6, [], ["amd"]);
      fireEvent.click(screen.getByTestId("tech-card-amd"));
      const ps = screen.getByTestId("tech-card-ps");
      expect(ps).toHaveAttribute("data-selectable", "false");
      expect(ps).toHaveAttribute("data-blocked-reason", "Not available to research");
    });
  });

  describe("filter", () => {
    it("narrows the cards to one colour and back", () => {
      renderWithIdentity(
        <TechnologyModal isOpen={true} onClose={vi.fn()} players={mockPlayers} viewerSeat="player1" />,
      );
      fireEvent.click(screen.getByTestId("tech-filter-biotic"));
      expect(screen.getByTestId("tech-card-nm")).toBeInTheDocument();
      expect(screen.queryByTestId("tech-card-amd")).toBeNull();
      expect(screen.queryByTestId("tech-card-cr2")).toBeNull();
      fireEvent.click(screen.getByTestId("tech-filter-unit_upgrade"));
      expect(screen.getByTestId("tech-card-cr2")).toBeInTheDocument();
      expect(screen.queryByTestId("tech-card-nm")).toBeNull();
      fireEvent.click(screen.getByTestId("tech-filter-all"));
      expect(screen.getByTestId("tech-card-amd")).toBeInTheDocument();
    });

    it("can show only what the viewer can research now", () => {
      renderWithIdentity(
        <TechnologyModal isOpen={true} onClose={vi.fn()} players={mockPlayers} viewerSeat="player1" />,
      );
      fireEvent.click(screen.getByTestId("tech-filter-available"));
      // Already researched (amd) and out of reach (War Sun, four prerequisites) are hidden.
      expect(screen.queryByTestId("tech-card-amd")).toBeNull();
      expect(screen.queryByTestId("tech-card-ws")).toBeNull();
      // A prerequisite-free technology the viewer lacks stays.
      expect(screen.getByTestId("tech-card-nm")).toBeInTheDocument();
    });
  });
});
