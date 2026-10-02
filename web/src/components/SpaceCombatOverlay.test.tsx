import { act } from "react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { SpaceCombatOverlay } from "./SpaceCombatOverlay.tsx";
import { PendingChoiceDto, BoardView, PlayerView } from "../protocol/types.ts";

describe("SpaceCombatOverlay", () => {
  const originalFetch = global.fetch;

  beforeEach(() => {
    global.fetch = vi.fn().mockImplementation(() => new Promise(() => {}));
  });

  afterEach(() => {
    global.fetch = originalFetch;
    vi.restoreAllMocks();
  });

  const sampleBoard: BoardView = {
    systems: {
      "18": {
        system_id: "18",
        command_tokens: [],
        planets: {},
        units: [
          // Attacker (seat_1): Dreadnought, Carrier, 2 Fighters
          { unit_type: "dreadnought", owner: "seat_1", damaged: false },
          { unit_type: "carrier", owner: "seat_1", damaged: true },
          { unit_type: "fighter", owner: "seat_1", damaged: false },
          { unit_type: "fighter", owner: "seat_1", damaged: false },
          // Defender (seat_2): Cruiser, Destroyer
          { unit_type: "cruiser", owner: "seat_2", damaged: false },
          { unit_type: "destroyer", owner: "seat_2", damaged: false },
        ],
      },
    },
    active_system: "18",
  };

  const samplePlayers: Record<string, PlayerView> = {
    seat_1: {
      id: "seat_1",
      faction: "Federation of Sol",
      victory_points: 4,
      trade_goods: 2,
      commodities: 3,
      tactic_tokens: 3,
      fleet_tokens: 3,
      strategic_tokens: 2,
      passed: false,
      strategy_cards: [],
      exhausted_strategy_cards: [],
      technologies: [],
      exhausted_technologies: [],
      relics: [],
      exhausted_relics: [],
      action_cards_count: 3,
      secret_objectives_count: 1,
      leaders: {},
    },
    seat_2: {
      id: "seat_2",
      faction: "Barony of Letnev",
      victory_points: 3,
      trade_goods: 1,
      commodities: 2,
      tactic_tokens: 2,
      fleet_tokens: 2,
      strategic_tokens: 1,
      passed: false,
      strategy_cards: [],
      exhausted_strategy_cards: [],
      technologies: [],
      exhausted_technologies: [],
      relics: [],
      exhausted_relics: [],
      action_cards_count: 2,
      secret_objectives_count: 1,
      leaders: {},
    },
  };

  it("renders both sides of combat with fleet supply and capacity gauges", () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    const sustainChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "100",
      prompt: "Sustain damage on a ship",
      context: {
        subtype: "sustain_damage",
        target: { System: "18" },
      },
      options: [
        { id: "sustain:dreadnought:1", label: "Dreadnought", kind: "sustain" },
        { id: "decline", label: "Take the hit", kind: "decline" },
      ],
    };

    render(
      <SpaceCombatOverlay
        isOpen={true}
        choice={sustainChoice}
        viewerSeat="seat_1"
        board={sampleBoard}
        players={samplePlayers}
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    // Both fleets present
    expect(screen.getByTestId("attacker-fleet-card")).toBeInTheDocument();
    expect(screen.getByTestId("defender-fleet-card")).toBeInTheDocument();

    // Attacker fleet supply: Dreadnought + Carrier = 2 non-fighters vs 3 fleet tokens
    expect(screen.getByTestId("attacker-fleet-supply-gauge")).toHaveTextContent("2 / 3");
    // Attacker capacity: Dreadnought (1) + Carrier (4) = 5 capacity; 2 Fighters = 2 used
    expect(screen.getByTestId("attacker-capacity-gauge")).toHaveTextContent("2 / 5");

    // Defender fleet supply: Cruiser + Destroyer = 2 non-fighters vs 2 fleet tokens
    expect(screen.getByTestId("defender-fleet-supply-gauge")).toHaveTextContent("2 / 2");
    // Defender capacity: 0 used / 0 capacity
    expect(screen.getByTestId("defender-capacity-gauge")).toHaveTextContent("0 / 0");

    // Damaged unit indicator on Carrier
    expect(screen.getByTestId("unit-row-carrier")).toHaveTextContent("0 + 1 damaged");

    expect(screen.queryByTestId("combat-odds-card")).not.toBeInTheDocument();
  });

  it("allows active player to sustain damage on eligible ship and handles submission", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    const sustainChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "101",
      prompt: "Sustain damage on a ship",
      context: {
        subtype: "sustain_damage",
        target: { System: "18" },
      },
      options: [
        { id: "sustain:dreadnought:1", label: "Dreadnought (System 18)", kind: "sustain" },
        { id: "decline", label: "Take the hit", kind: "decline" },
      ],
    };

    render(
      <SpaceCombatOverlay
        isOpen={true}
        choice={sustainChoice}
        viewerSeat="seat_1"
        board={sampleBoard}
        players={samplePlayers}
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    expect(screen.queryByText(/Caution: Opponents holding "Direct Hit"/i)).not.toBeInTheDocument();
    const sustainBtn = screen.getByTestId("sustain-opt-sustain:dreadnought:1");
    expect(sustainBtn).toBeInTheDocument();

    await act(async () => {
      fireEvent.click(sustainBtn);
    });

    expect(onSubmit).toHaveBeenCalledWith("sustain:dreadnought:1");
  });

  it("summarizes hits by ship type and side with full rolls available on focus", () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "102",
      prompt: "Assign hits",
      context: {
        subtype: "assign_casualty",
        outstanding: [{ amount: 2 }],
        target: { System: "18" },
      },
      options: [{ id: "destroy|fighter", label: "Destroy Fighter", kind: "casualty" }],
    };

    const diceRolls = [
      { player: "seat_1", unit: "Dreadnought", roll: 8, target: 5, hit: true },
      { player: "seat_1", unit: "dreadnought", roll: 2, target: 5, hit: false },
      { player: "seat_1", unit: "Carrier", roll: 9, target: 9, hit: true },
      { player: "seat_2", unit: "Cruiser", roll: 7, target: 7, hit: true },
      { player: "seat_2", unit: "Destroyer", roll: 3, target: 9, hit: false },
    ];

    render(
      <SpaceCombatOverlay
        isOpen={true}
        choice={choice}
        viewerSeat="seat_1"
        board={sampleBoard}
        players={samplePlayers}
        recentDiceRolls={diceRolls}
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    const dreadnoughts = screen.getByTestId("combat-roll-group-seat_1-dreadnought");
    expect(dreadnoughts).toHaveTextContent("1 hit");
    expect(dreadnoughts.closest("[data-testid='unit-row-dreadnought']")).toBeInTheDocument();
    expect(dreadnoughts).toHaveAttribute(
      "aria-label",
      "Dreadnought: 1 hit from 2 rolls. 8 (5+) hit, 2 (5+) miss",
    );
    expect(dreadnoughts.querySelectorAll(".combat-unit-row__roll-result")).toHaveLength(2);
    expect(screen.getByTestId("combat-roll-group-seat_1-carrier")).toHaveTextContent("1 hit");
    expect(screen.getByTestId("combat-roll-group-seat_1-fighter")).toHaveAttribute(
      "aria-label",
      "Fighter: 0 hits from 0 rolls.",
    );
    expect(screen.getByTestId("combat-roll-group-seat_1-fighter")).toHaveTextContent("0 hits");
    expect(screen.getByTestId("combat-roll-group-seat_2-cruiser")).toHaveTextContent("1 hit");
    expect(screen.getByTestId("combat-roll-group-seat_2-destroyer")).toHaveTextContent("0 hits");
    expect(screen.queryByTestId("combat-dice-feed")).not.toBeInTheDocument();
    expect(screen.getByTestId("combat-hits-callout")).toHaveTextContent("2");
  });

  it("renders dockable minimized bar when isMinimized is true", () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onMinimize = vi.fn();

    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "103",
      prompt: "Assign hits",
      context: {
        subtype: "assign_casualty",
        target: { System: "18" },
      },
      options: [],
    };

    render(
      <SpaceCombatOverlay
        isOpen={true}
        choice={choice}
        viewerSeat="seat_1"
        board={sampleBoard}
        players={samplePlayers}
        isMinimized={true}
        onMinimize={onMinimize}
        onSubmit={onSubmit}
        onClose={() => {}}
      />,
    );

    expect(screen.getByTestId("combat-docked-pill")).toBeInTheDocument();
    expect(screen.getByText(/SPACE COMBAT/i)).toBeInTheDocument();
    expect(screen.getByText(/System 18/i)).toBeInTheDocument();

    const resumeBtn = screen.getByTestId("resume-combat-btn");
    expect(resumeBtn).toBeInTheDocument();
    fireEvent.click(resumeBtn);
    expect(onMinimize).toHaveBeenCalledWith(false);
  });

  it("renders in spectator mode for observers and opponents when not their turn", () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);

    const choice: PendingChoiceDto = {
      actor: "seat_2",
      nonce: "104",
      prompt: "Sustain damage",
      context: {
        subtype: "sustain_damage",
        target: { System: "18" },
      },
      options: [{ id: "sustain:cruiser", label: "Cruiser", kind: "sustain" }],
    };

    render(
      <SpaceCombatOverlay
        isOpen={true}
        choice={choice}
        viewerSeat="seat_1"
        board={sampleBoard}
        players={samplePlayers}
        onSubmit={onSubmit}
        onClose={() => {}}
      />,
    );

    expect(screen.getByTestId("spectator-combat-notice")).toBeInTheDocument();
    // Action buttons are NOT rendered for the spectator
    expect(screen.queryByTestId("sustain-opt-sustain:cruiser")).not.toBeInTheDocument();
  });

  it("fetches and renders live simulated combat odds from advisor endpoint", async () => {
    const mockOdds = {
      simulations: 2000,
      attacker_win_rate: 0.72,
      defender_win_rate: 0.25,
      mutual_destruction_rate: 0.03,
      unresolved_rate: 0,
      average_rounds: 2.4,
      attacker_expected_survivors: { carrier: 0.8, dreadnought: 0.9, fighter: 0.3 },
      defender_expected_survivors: { cruiser: 0.2 },
      attacker_fielded: { carrier: 1, dreadnought: 1, fighter: 2 },
      defender_fielded: { cruiser: 1, destroyer: 1 },
    };

    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => mockOdds,
    } as Response);

    const sustainChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "105",
      prompt: "Sustain damage on a ship",
      context: {
        subtype: "sustain_damage",
        target: { System: "18" },
      },
      options: [
        { id: "sustain:dreadnought:1", label: "Dreadnought", kind: "sustain" },
        { id: "decline", label: "Take the hit", kind: "decline" },
      ],
    };

    await act(async () => {
      render(
        <SpaceCombatOverlay
          isOpen={true}
          choice={sustainChoice}
          viewerSeat="seat_1"
          board={{
            ...sampleBoard,
            combat: {
              system_id: "18",
              round: 1,
              battle_seq: 1,
              phase: "pre_roll",
              attacker: "seat_1",
              defender: "seat_2",
            },
          }}
          players={samplePlayers}
          onSubmit={vi.fn().mockResolvedValue(undefined)}
          onClose={vi.fn()}
        />,
      );
    });

    expect(screen.getByTestId("combat-odds-tag")).toHaveTextContent("Simulated (2,000 rollouts)");
    expect(screen.getByText("72%")).toBeInTheDocument();
    expect(screen.getByText("25%")).toBeInTheDocument();
    expect(screen.getByText(/~2.0 survivors/i)).toBeInTheDocument();
    expect(screen.getByText(/~0.2 survivors/i)).toBeInTheDocument();
    expect(screen.getByTestId("combat-odds-sub-detail")).toHaveTextContent(
      "Avg 2.4 rounds • 3% mutual wipe",
    );

    expect(global.fetch).toHaveBeenCalledWith(
      "/advisor/battle",
      expect.objectContaining({
        method: "POST",
        body: expect.stringContaining('"faction":"sol"'),
      }),
    );
  });

  it("labels the rough fleet estimate when the advisor fails", async () => {
    global.fetch = vi.fn().mockRejectedValue(new Error("Advisor offline"));

    const sustainChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "106",
      prompt: "Sustain damage on a ship",
      context: {
        subtype: "sustain_damage",
        target: { System: "18" },
      },
      options: [{ id: "sustain:dreadnought:1", label: "Dreadnought", kind: "sustain" }],
    };

    await act(async () => {
      render(
        <SpaceCombatOverlay
          isOpen={true}
          choice={sustainChoice}
          viewerSeat="seat_1"
          board={{
            ...sampleBoard,
            combat: {
              system_id: "18",
              round: 1,
              battle_seq: 1,
              phase: "pre_roll",
              attacker: "seat_1",
              defender: "seat_2",
            },
          }}
          players={samplePlayers}
          onSubmit={vi.fn().mockResolvedValue(undefined)}
          onClose={vi.fn()}
        />,
      );
    });

    expect(screen.getByTestId("combat-odds-tag")).toHaveTextContent(
      "Rough fleet estimate · advisor unavailable",
    );
    expect(screen.getByTestId("combat-odds-card")).toBeInTheDocument();
    expect(screen.getAllByText(/\d+%/)).toHaveLength(2);
  });

  it("never displays late odds after rolls or for another round", async () => {
    const replies: ((value: Response) => void)[] = [];
    global.fetch = vi
      .fn()
      .mockImplementation(() => new Promise<Response>((resolve) => replies.push(resolve)));
    const combat = {
      system_id: "18",
      round: 1,
      battle_seq: 3,
      phase: "pre_roll" as const,
      attacker: "seat_1",
      defender: "seat_2",
    };
    const props = {
      isOpen: true,
      choice: null,
      viewerSeat: "seat_1",
      players: samplePlayers,
      onSubmit: vi.fn(),
      onClose: vi.fn(),
    };
    const { rerender } = render(
      <SpaceCombatOverlay {...props} board={{ ...sampleBoard, combat }} />,
    );
    expect(screen.getByTestId("combat-odds-tag")).toHaveTextContent(
      "Rough fleet estimate · simulation pending",
    );
    expect(screen.getByTestId("combat-odds-card")).toHaveTextContent(/\d+%/);
    expect(screen.getAllByText(/\d+%/)).toHaveLength(2);
    rerender(
      <SpaceCombatOverlay
        {...props}
        board={{ ...sampleBoard, combat: { ...combat, phase: "resolving_hits" } }}
      />,
    );
    expect(screen.queryByTestId("combat-odds-card")).not.toBeInTheDocument();
    rerender(
      <SpaceCombatOverlay {...props} board={{ ...sampleBoard, combat: { ...combat, round: 2 } }} />,
    );
    expect(screen.getByTestId("combat-odds-tag")).toHaveTextContent(
      "Rough fleet estimate · simulation pending",
    );
    expect(screen.getByTestId("combat-odds-card")).toHaveTextContent(/\d+%/);
    await act(async () =>
      replies[0]({ ok: true, json: async () => ({ attacker_win_rate: 0.9 }) } as Response),
    );
    expect(screen.queryByText("90%")).not.toBeInTheDocument();
  });

  it("compares damage and casualties to the round-start fleet without hiding survivors", () => {
    const start = sampleBoard.systems["18"].units;
    const combat = {
      system_id: "18",
      round: 2,
      battle_seq: 5,
      phase: "resolving_hits" as const,
      attacker: "seat_1",
      defender: "seat_2",
      round_start: start,
      attacker_hits: 2,
      defender_hits: 1,
    };
    const board: BoardView = {
      ...sampleBoard,
      combat,
      systems: {
        "18": {
          ...sampleBoard.systems["18"],
          units: start
            .filter((_, index) => index !== 2)
            .map((unit) => (unit.unit_type === "dreadnought" ? { ...unit, damaged: true } : unit)),
        },
      },
    };
    const { rerender } = render(
      <SpaceCombatOverlay
        isOpen
        choice={null}
        board={board}
        players={samplePlayers}
        onSubmit={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByTestId("combat-new-damage-seat_1-dreadnought")).toHaveTextContent(
      "0 + 1 damaged",
    );
    expect(screen.getByTestId("combat-destroyed-seat_1-fighter")).toHaveTextContent("−1");
    expect(screen.getByTestId("unit-row-dreadnought")).toHaveTextContent("0 + 1 damaged");
    rerender(
      <SpaceCombatOverlay
        isOpen
        choice={null}
        board={{
          ...board,
          combat: {
            ...combat,
            phase: "pre_roll",
            round: 3,
            round_start: board.systems["18"].units,
            dice_rolls: [],
          },
        }}
        players={samplePlayers}
        onSubmit={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.queryByTestId("combat-new-damage-seat_1-dreadnought")).not.toBeInTheDocument();
    expect(screen.queryByTestId("combat-destroyed-seat_1-fighter")).not.toBeInTheDocument();
    expect(screen.getByTestId("unit-row-dreadnought")).toHaveTextContent("1 damaged");
  });

  it("updates the pill after each sustain on two ships without changing the fleet count", () => {
    const dreadnoughts = [
      { unit_type: "dreadnought", owner: "seat_2", damaged: false },
      { unit_type: "dreadnought", owner: "seat_2", damaged: false },
    ];
    const combat = {
      system_id: "18",
      round: 1,
      battle_seq: 9,
      phase: "resolving_hits" as const,
      attacker: "seat_1",
      defender: "seat_2",
      round_start: dreadnoughts,
    };
    const props = {
      isOpen: true,
      choice: null,
      players: samplePlayers,
      onSubmit: vi.fn(),
      onClose: vi.fn(),
    };
    const boardWith = (damaged: number): BoardView => ({
      ...sampleBoard,
      combat,
      systems: {
        "18": {
          ...sampleBoard.systems["18"],
          units: dreadnoughts.map((unit, index) => ({ ...unit, damaged: index < damaged })),
        },
      },
    });
    const { rerender } = render(<SpaceCombatOverlay {...props} board={boardWith(0)} />);
    const row = screen
      .getByTestId("defender-fleet-card")
      .querySelector('[data-testid="unit-row-dreadnought"]')!;
    expect(row).toHaveTextContent("×2");
    expect(row.querySelector(".combat-unit-row__damaged-badge")).not.toBeInTheDocument();

    rerender(<SpaceCombatOverlay {...props} board={boardWith(1)} />);
    expect(row).toHaveTextContent("×2");
    expect(row.querySelector(".combat-unit-row__damaged-badge")).toHaveTextContent("0 + 1 damaged");

    rerender(<SpaceCombatOverlay {...props} board={boardWith(2)} />);
    expect(row).toHaveTextContent("×2");
    expect(row.querySelector(".combat-unit-row__damaged-badge")).toHaveTextContent("0 + 2 damaged");
    expect(screen.queryByTestId("combat-odds-card")).not.toBeInTheDocument();
  });

  it("keeps automatically assigned barrage losses visible in the same fleet view before rolls", () => {
    const start = sampleBoard.systems["18"].units;
    const board: BoardView = {
      ...sampleBoard,
      systems: {
        "18": { ...sampleBoard.systems["18"], units: start.filter((_, index) => index !== 2) },
      },
      combat: {
        system_id: "18",
        round: 1,
        battle_seq: 4,
        phase: "pre_roll",
        attacker: "seat_1",
        defender: "seat_2",
        barrage_start: start,
        barrage_hits: { seat_1: 0, seat_2: 1 },
        barrage_dice: [{ player: "seat_2", unit: "destroyer", roll: 10, target: 9, hit: true }],
      },
    };
    render(
      <SpaceCombatOverlay
        isOpen
        choice={null}
        board={board}
        onSubmit={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByTestId("combat-barrage-results")).toHaveTextContent(
      "automatically destroy opposing fighters",
    );
    expect(screen.getByTestId("combat-round-hits")).toHaveTextContent(
      "Anti-fighter barrage Hits Produced",
    );
    expect(screen.getByTestId("attacker-round-hits")).toHaveTextContent("0");
    expect(screen.getByTestId("defender-round-hits")).toHaveTextContent("1");
    expect(screen.getByTestId("unit-row-fighter")).toHaveTextContent("×1−1");
    expect(screen.queryByTestId("combat-remaining-hits")).not.toBeInTheDocument();
  });

  it("closes rather than minimizes when combat is complete", () => {
    const onClose = vi.fn();
    const onMinimize = vi.fn();
    render(
      <SpaceCombatOverlay
        isOpen
        choice={null}
        board={{
          ...sampleBoard,
          combat: {
            system_id: "18",
            round: 1,
            phase: "complete",
            attacker: "seat_1",
            defender: "seat_2",
          },
        }}
        onSubmit={vi.fn()}
        onClose={onClose}
        onMinimize={onMinimize}
      />,
    );
    const close = screen.getByRole("button", { name: "Close combat" });
    expect(close).toHaveTextContent("×");
    fireEvent.click(close);
    expect(onClose).toHaveBeenCalledOnce();
    expect(onMinimize).not.toHaveBeenCalled();
  });

  it("presents a reaction card with a title, description and explicit play action", () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(
      <SpaceCombatOverlay
        isOpen
        viewerSeat="seat_1"
        choice={{
          actor: "seat_1",
          nonce: "reaction",
          prompt: "React to combat",
          context: { subtype: "reaction_SPACE_COMBAT_STARTED", target: { System: "18" } },
          options: [
            { id: "play", label: "Card", kind: "reaction", payload: { card: "direct_hit" } },
            { id: "decline", label: "Pass", kind: "decline" },
          ],
        }}
        board={sampleBoard}
        onSubmit={onSubmit}
        onClose={vi.fn()}
      />,
    );
    const offer = screen.getByText("ACTION CARD").closest(".combat-card-offer");
    expect(offer).toHaveTextContent("Direct Hit");
    expect(offer?.querySelector(".combat-card-offer__description")).not.toBeEmptyDOMElement();
    fireEvent.click(screen.getByRole("button", { name: "Play Direct Hit" }));
    expect(onSubmit).toHaveBeenCalledWith("play");
  });

  it("renders round hits scorecard displaying hits dealt by each player", () => {
    const boardWithCombat: BoardView = {
      ...sampleBoard,
      combat: {
        system_id: "18",
        round: 2,
        phase: "resolving_hits",
        attacker: "seat_1",
        defender: "seat_2",
        attacker_hits: 3,
        defender_hits: 1,
        hits_to_assign: 1,
        dice_rolls: [
          { player: "seat_1", unit: "War Sun", roll: 7, target: 3, hit: true },
          { player: "seat_1", unit: "War Sun", roll: 8, target: 3, hit: true },
        ],
      },
    };

    render(
      <SpaceCombatOverlay
        isOpen={true}
        choice={null}
        viewerSeat="seat_1"
        board={boardWithCombat}
        players={samplePlayers}
        onSubmit={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByTestId("combat-round-hits")).toBeInTheDocument();
    expect(screen.getByTestId("combat-stage-title")).toHaveTextContent(
      "Space Combat · System 18 · Round 2",
    );
    expect(screen.getByText("Round 2 Hits Produced")).toBeInTheDocument();
    expect(screen.getByTestId("attacker-round-hits")).toHaveTextContent("3");
    expect(screen.getByTestId("defender-round-hits")).toHaveTextContent("1");
    expect(screen.getByTestId("attacker-hits-dealt")).toHaveTextContent("💥 3 hits");
    expect(screen.getByTestId("defender-hits-dealt")).toHaveTextContent("💥 1 hit");
    expect(screen.getByTestId("unit-row-lost-warsun")).toHaveTextContent("War Sun×02 hits");
  });

  it("keeps casualty rows in place when the board reorders ships or loses a type", () => {
    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "assign-1",
      prompt: "Assign a hit",
      context: { subtype: "assign_casualty", target: { System: "18" } },
      options: [
        { id: "fighter", label: "fighter", kind: "casualty", payload: { unit: "fighter" } },
        {
          id: "dreadnought",
          label: "dreadnought",
          kind: "casualty",
          payload: { unit: "dreadnought" },
        },
      ],
    };
    const board: BoardView = {
      ...sampleBoard,
      combat: {
        system_id: "18",
        round: 1,
        attacker: "seat_1",
        defender: "seat_2",
        attacker_hits: 1,
        defender_hits: 2,
        hits_to_assign: 2,
        dice_rolls: [{ player: "seat_1", unit: "dreadnought", roll: 9, target: 5, hit: true }],
      },
    };
    const props = {
      isOpen: true,
      choice,
      viewerSeat: "seat_1",
      players: samplePlayers,
      onSubmit: vi.fn().mockResolvedValue(undefined),
      onClose: vi.fn(),
    };
    const order = () =>
      Array.from(screen.getByTestId("attacker-units-list").children).map((row) =>
        row.getAttribute("data-testid")?.replace("unit-row-lost-", "unit-row-"),
      );
    const { rerender } = render(<SpaceCombatOverlay {...props} board={board} />);
    const initialOrder = order();
    const remaining = board.systems["18"].units.filter(
      (unit) => unit.owner !== "seat_1" || unit.unit_type !== "fighter",
    );
    rerender(
      <SpaceCombatOverlay
        {...props}
        choice={{ ...choice, nonce: "assign-2" }}
        board={{
          ...board,
          systems: { "18": { ...board.systems["18"], units: remaining.reverse() } },
        }}
      />,
    );
    expect(order()).toEqual(initialOrder);
    expect(screen.getByTestId("unit-row-lost-fighter")).toHaveTextContent("×0");
  });

  it("shows the viewer's action cards and only the opponent's card count", () => {
    render(
      <SpaceCombatOverlay
        isOpen
        choice={null}
        viewerSeat="seat_1"
        board={sampleBoard}
        players={{
          seat_1: {
            ...samplePlayers.seat_1,
            held_action_cards: ["direct_hit"],
            action_cards_count: 1,
          },
          seat_2: {
            ...samplePlayers.seat_2,
            held_action_cards: ["sabotage"],
            action_cards_count: 3,
          },
        }}
        onSubmit={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByTestId("combat-cards-seat_1")).toHaveTextContent("Action cards: 1");
    expect(screen.getByTestId("combat-cards-seat_1")).toHaveTextContent("Direct Hit");
    expect(screen.getByTestId("combat-cards-seat_2")).toHaveTextContent("Action cards: 3");
    expect(screen.getByTestId("combat-cards-seat_2")).not.toHaveTextContent("Sabotage");
  });

  it("allows player to assign casualty by clicking directly on the unit row", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const casualtyChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "107",
      prompt: "Assign casualty",
      context: {
        subtype: "assign_casualty",
        target: { System: "18" },
      },
      options: [
        {
          id: "destroy|18|carrier|true",
          label: "Carrier (damaged)",
          kind: "casualty",
          payload: { unit: "carrier", damaged: true },
        },
        {
          id: "destroy|18|fighter|false",
          label: "Fighter",
          kind: "casualty",
          payload: { unit: "fighter" },
        },
      ],
    };

    render(
      <SpaceCombatOverlay
        isOpen={true}
        choice={casualtyChoice}
        viewerSeat="seat_1"
        board={sampleBoard}
        players={samplePlayers}
        recentDiceRolls={[{ player: "seat_1", unit: "carrier", roll: 10, target: 9, hit: true }]}
        onSubmit={onSubmit}
        onClose={vi.fn()}
      />,
    );

    // Find the carrier unit row
    const carrierRow = screen.getByTestId("unit-row-carrier");
    expect(carrierRow).toHaveClass("combat-unit-row--interactive");
    expect(carrierRow).toHaveTextContent("💥 Click to assign");
    expect(screen.queryByText(/hits remaining.*Click a ship row/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/Location: System 18/i)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Assign this hit/i })).not.toBeInTheDocument();
    expect(screen.queryByText(/Assign this hit:/i)).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId("combat-roll-group-seat_1-carrier"));
    expect(onSubmit).not.toHaveBeenCalled();

    // Click directly on the carrier row
    await act(async () => {
      fireEvent.click(carrierRow);
    });

    expect(onSubmit).toHaveBeenCalledWith("destroy|18|carrier|true");
  });

  it("renders Direct Hit warning when opponent holds Direct Hit card", () => {
    const playersWithDirectHit: Record<string, PlayerView> = {
      ...samplePlayers,
      seat_2: {
        ...samplePlayers.seat_2,
        held_action_cards: ["direct_hit"],
      },
    };

    const sustainChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "108",
      prompt: "Sustain damage",
      context: {
        subtype: "sustain_damage",
        target: { System: "18" },
      },
      options: [
        { id: "sustain:dreadnought:1", label: "Dreadnought", kind: "sustain" },
        { id: "decline", label: "Decline", kind: "decline" },
      ],
    };

    render(
      <SpaceCombatOverlay
        isOpen={true}
        choice={sustainChoice}
        viewerSeat="seat_1"
        board={sampleBoard}
        players={playersWithDirectHit}
        onSubmit={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByTestId("direct-hit-threat-banner")).toBeInTheDocument();
    expect(screen.getByTestId("direct-hit-threat-banner")).toHaveTextContent(
      "Opponent holds Direct Hit — sustained ships may be destroyed.",
    );
    expect(screen.queryByText(/Caution: Opponents holding/i)).not.toBeInTheDocument();
    expect(screen.queryByTestId("direct-hit-held-banner")).not.toBeInTheDocument();
  });
});
