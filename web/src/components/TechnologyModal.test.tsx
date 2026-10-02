import React from "react";
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { TechnologyModal } from "./TechnologyModal.tsx";
import { PlayerIdentityProvider } from "../presentation/PlayerIdentity.tsx";
import type { LobbyDto, PlayerView, BoardView, PendingChoiceDto } from "../protocol/types.ts";

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
      ready: true,
      connected: true,
      can_take_over: false,
    },
    {
      slot_id: "s2",
      position: 2,
      occupant: "player2",
      nickname: "Bob",
      ready: true,
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

  it("displays player markers with symbol, position/nickname, and seat color for researched tech", () => {
    renderWithIdentity(<TechnologyModal isOpen={true} onClose={vi.fn()} players={mockPlayers} />);

    // Player 1 researched AMD
    const amdP1Marker = screen.getByTestId("player-tech-marker-player1-amd");
    expect(amdP1Marker).toBeInTheDocument();
    // Position 1 color in SEAT_COLORS is #E69F00, symbol is ●
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
      context: {
        subtype: "research_technology",
        source: {
          StrategyCard: {
            card: "Technology",
            secondary: false,
          },
        },
      },
      options: [
        { id: "amd", label: "Antimass Deflectors", kind: "research" },
        { id: "ps", label: "Plasma Scoring", kind: "research" },
        { id: "decline", label: "Decline", kind: "decline" },
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
    expect(screen.getByText("Cost: 6 Resources")).toBeInTheDocument();
    expect(confirmBtn).toHaveTextContent("Confirm Research (2 Techs - 6 Resources)");

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
      context: {
        subtype: "research_technology",
        source: {
          StrategyCard: {
            card: "Technology",
            secondary: false,
          },
        },
      },
      options: [
        { id: "st", label: "Sarween Tools", kind: "research" },
        { id: "ps", label: "Plasma Scoring", kind: "research" },
        { id: "decline", label: "Decline", kind: "decline" },
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
});
