import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ChoiceRendererDispatcher, GameShell } from "./GameShell.tsx";
import { BoardView, PendingChoiceDto } from "../protocol/types.ts";
import { deriveChoiceRendererModel } from "../presentation/choiceModel.ts";
import { PlayerIdentityProvider } from "../presentation/PlayerIdentity.tsx";
import type { LobbyDto } from "../protocol/types.ts";

const choice: PendingChoiceDto = {
  prompt: "Choose a strategy card",
  actor: "p1",
  nonce: "choice-1",
  options: [{ id: "leadership", label: "Leadership" }],
};

function renderShell(pendingChoice: PendingChoiceDto | null = null) {
  return render(
    <GameShell
      header={<div>Header</div>}
      board={<div data-testid="board-content">Board</div>}
      playerSheet={<div>Player sheet</div>}
      events={[]}
      choice={pendingChoice}
      onSubmitChoice={vi.fn().mockResolvedValue(undefined)}
    />,
  );
}

describe("GameShell", () => {
  it("routes only battle-associated reactions and their card selection into the overlay", async () => {
    const board: BoardView = {
      systems: {
        "18": {
          system_id: "18",
          command_tokens: [],
          planets: {},
          units: [
            { owner: "p1", unit_type: "cruiser", damaged: false },
            { owner: "p2", unit_type: "cruiser", damaged: false },
          ],
        },
      },
      combat: { system_id: "18", round: 1, attacker: "p1", defender: "p2", dice_rolls: [] },
    };
    const submit = vi.fn().mockResolvedValue(undefined);
    const offer: PendingChoiceDto = {
      actor: "p2",
      nonce: "outer",
      prompt: "when HITS_TO_ASSIGN",
      context: { subtype: "reaction_when_HITS_TO_ASSIGN", space_battle: true },
      options: [
        {
          id: "reaction:p2:HITS_TO_ASSIGN:when",
          kind: "ability",
          label: "Play Shields Holding",
          payload: { card: "sh1" },
        },
        { id: "decline", kind: "decline", label: "Decline" },
      ],
    };
    const props = {
      viewerSeat: "p2",
      boardView: board,
      onSubmit: submit,
      isMinimized: false,
      onMinimizedChange: vi.fn(),
    };
    const { rerender } = render(<ChoiceRendererDispatcher {...props} choice={offer} />);
    expect(screen.getByTestId("combat-resolution-modal")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Play Shields Holding" }));
    await waitFor(() => expect(submit).toHaveBeenCalledWith(offer.options[0].id));
    rerender(
      <ChoiceRendererDispatcher
        {...props}
        choice={{
          ...offer,
          nonce: "inner",
          context: { subtype: "play_reaction_when_HITS_TO_ASSIGN", space_battle: true },
          options: [
            {
              id: "sh1",
              kind: "action_card",
              label: "play Shields Holding",
              payload: { card: "sh1" },
            },
          ],
        }}
      />,
    );
    expect(screen.getByRole("button", { name: "Play Shields Holding" })).toBeInTheDocument();
    rerender(
      <ChoiceRendererDispatcher
        {...props}
        choice={{
          ...offer,
          context: { subtype: "reaction_when_HITS_TO_ASSIGN", space_battle: false },
        }}
        boardView={{ ...board, combat: undefined }}
      />,
    );
    expect(screen.queryByTestId("combat-resolution-modal")).not.toBeInTheDocument();
    expect(screen.getByTestId("pending-choice-dialog")).toBeInTheDocument();
  });

  it("hands off the next decision immediately after combat", () => {
    const activeBoard: BoardView = {
      systems: {
        "18": {
          system_id: "18",
          command_tokens: [],
          planets: {},
          units: [
            { owner: "p1", unit_type: "dreadnought", damaged: false },
            { owner: "p2", unit_type: "cruiser", damaged: false },
          ],
        },
      },
      combat: {
        system_id: "18",
        round: 2,
        attacker: "p1",
        defender: "p2",
        attacker_hits: 1,
        defender_hits: 0,
        dice_rolls: [{ player: "p1", unit: "dreadnought", roll: 9, target: 5, hit: true }],
      },
    };
    const nextBoard: BoardView = {
      systems: {
        "18": { ...activeBoard.systems["18"], units: activeBoard.systems["18"].units.slice(0, 1) },
      },
    };
    const combatChoice: PendingChoiceDto = {
      actor: "p2",
      nonce: "combat-last",
      prompt: "Assign a casualty",
      context: { subtype: "assign_casualty", target: { System: "18" } },
      options: [{ id: "destroy", label: "Cruiser", kind: "casualty" }],
    };
    const props = {
      header: <div>Header</div>,
      board: <div>Board</div>,
      playerSheet: <div>Player sheet</div>,
      events: [],
      viewerSeat: "p1",
      onSubmitChoice: vi.fn().mockResolvedValue(undefined),
    };
    const { rerender } = render(
      <GameShell {...props} choice={combatChoice} boardView={activeBoard} />,
    );
    expect(screen.getByTestId("combat-resolution-modal")).toBeInTheDocument();

    rerender(<GameShell {...props} choice={choice} boardView={nextBoard} />);
    expect(screen.queryByTestId("combat-resolution-modal")).not.toBeInTheDocument();
    expect(screen.getByTestId("pending-choice-dialog")).toBeInTheDocument();

    rerender(
      <GameShell {...props} choice={{ ...choice, nonce: "next-choice" }} boardView={nextBoard} />,
    );
    expect(screen.queryByTestId("combat-resolution-modal")).not.toBeInTheDocument();
    expect(screen.getByTestId("pending-choice-dialog")).toBeInTheDocument();
  });

  it("does not restore completed combat after a page reload", () => {
    const board: BoardView = {
      systems: { "18": { system_id: "18", command_tokens: [], planets: {}, units: [] } },
      combat: { system_id: "18", round: 1, attacker: "p1", defender: "p2" },
    };
    const props = {
      header: <div>Header</div>,
      board: <div>Board</div>,
      playerSheet: <div>Player sheet</div>,
      events: [],
      choice: null,
      onSubmitChoice: vi.fn().mockResolvedValue(undefined),
    };
    const { rerender, unmount } = render(<GameShell {...props} boardView={board} />);
    rerender(<GameShell {...props} boardView={{ ...board, combat: undefined }} />);
    expect(screen.queryByTestId("combat-resolution-modal")).not.toBeInTheDocument();
    unmount();
    render(<GameShell {...props} boardView={{ ...board, combat: undefined }} />);
    expect(screen.queryByTestId("combat-resolution-modal")).not.toBeInTheDocument();
  });

  it.each([
    {
      name: "payment",
      first: {
        actor: "p1",
        nonce: "one",
        prompt: "pay 2 resources",
        context: {
          subtype: "pay_resources",
          outstanding: [{ kind: "resources", amount: 2, paid: 0 }],
        },
        options: [
          {
            id: "exhaust|jord",
            kind: "pay",
            label: "Jord",
            payload: { worth: 1, planet_name: "Jord" },
          },
          { id: "trade_good", kind: "pay", label: "Trade good", payload: { worth: 1 } },
        ],
      } satisfies PendingChoiceDto,
      second: { id: "trade_good", kind: "pay", label: "Trade good", payload: { worth: 1 } },
      expected: ["exhaust|jord", "trade_good"],
      stage: () => {
        fireEvent.click(screen.getByTestId("planet-card-exhaust|jord").querySelector("input")!);
        fireEvent.click(screen.getByTestId("tg-increment-btn"));
        fireEvent.click(screen.getByTestId("confirm-payment-btn"));
      },
    },
    {
      name: "cargo",
      first: {
        actor: "p1",
        nonce: "one",
        prompt: "load carrier (2 free)",
        context: { subtype: "load_cargo", target: { System: "42" } },
        options: [
          {
            id: "load|0",
            kind: "load",
            label: "Fighter",
            payload: { unit: "fighter", source: null, capacity_remaining: 2 },
          },
          { id: "done_loading", kind: "decline", label: "Done" },
        ],
      } satisfies PendingChoiceDto,
      second: {
        id: "load|1",
        kind: "load",
        label: "Fighter",
        payload: { unit: "fighter", source: null, capacity_remaining: 1 },
      },
      expected: ["load|0", "load|1"],
      stage: () => {
        fireEvent.click(screen.getByRole("button", { name: "Stage fighter from space" }));
        fireEvent.click(screen.getByRole("button", { name: "Stage fighter from space" }));
        fireEvent.click(screen.getByRole("button", { name: "Confirm 2 loads" }));
      },
    },
    {
      name: "agenda voting",
      first: {
        actor: "p1",
        nonce: "one",
        prompt: "exhaust planets to vote",
        context: { subtype: "vote_exhaust_planet" },
        options: [
          { id: "Jord", kind: "vote_planet", label: "Jord", payload: { votes: 2 } },
          { id: "Mecatol", kind: "vote_planet", label: "Mecatol", payload: { votes: 6 } },
          { id: "decline", kind: "decline", label: "Done" },
        ],
      } satisfies PendingChoiceDto,
      second: { id: "Mecatol", kind: "vote_planet", label: "Mecatol", payload: { votes: 6 } },
      expected: ["Jord", "Mecatol"],
      stage: () => {
        fireEvent.click(screen.getByTestId("planet-card-Jord"));
        fireEvent.click(screen.getByTestId("planet-card-Mecatol"));
        fireEvent.click(screen.getByTestId("commit-planet-votes-btn"));
      },
    },
    {
      name: "generic multi-select",
      first: {
        actor: "p1",
        nonce: "one",
        prompt: "choose two",
        context: {
          subtype: "select_targets",
          outstanding: [{ min_selection: 1, max_selection: 2 }],
        },
        options: [
          { id: "first", kind: "target", label: "First" },
          { id: "second", kind: "target", label: "Second" },
        ],
      } satisfies PendingChoiceDto,
      second: { id: "second", kind: "target", label: "Second" },
      expected: ["first", "second"],
      stage: () => {
        fireEvent.click(screen.getByText("First"));
        fireEvent.click(screen.getByText("Second"));
        fireEvent.click(screen.getByTestId("submit-choice-button"));
      },
    },
  ])(
    "resumes the $name queue after its modal unmounts between choices",
    async ({ first, second, expected, stage }) => {
      const onSubmit = vi.fn().mockResolvedValue(undefined);
      const view = (pending: PendingChoiceDto | null) => (
        <GameShell
          header={null}
          board={null}
          playerSheet={null}
          events={[]}
          choice={pending}
          viewerSeat="p1"
          onSubmitChoice={onSubmit}
          boardView={{
            systems: {
              "42": {
                system_id: "42",
                command_tokens: [],
                planets: {},
                units: [
                  { owner: "p1", unit_type: "fighter", damaged: false },
                  { owner: "p1", unit_type: "fighter", damaged: false },
                ],
              },
            },
          }}
        />
      );
      const { rerender } = render(view(first));
      stage();
      await waitFor(() => expect(onSubmit).toHaveBeenCalledExactlyOnceWith(expected[0]));
      rerender(view(null));
      expect(screen.queryByTestId("decision-modal")).not.toBeInTheDocument();
      expect(onSubmit).toHaveBeenCalledTimes(1);
      rerender(view({ ...first, nonce: "two", options: [second] }));
      await waitFor(() => expect(onSubmit).toHaveBeenNthCalledWith(2, expected[1]));
    },
  );

  it("cancels an interrupted queue rather than submitting to a different workflow", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const first: PendingChoiceDto = {
      actor: "p1",
      nonce: "one",
      prompt: "choose two",
      context: { subtype: "select_targets", outstanding: [{ min_selection: 1, max_selection: 2 }] },
      options: [
        { id: "first", label: "First" },
        { id: "second", label: "Second" },
      ],
    };
    const view = (pending: PendingChoiceDto | null) => (
      <GameShell
        header={null}
        board={null}
        playerSheet={null}
        events={[]}
        choice={pending}
        viewerSeat="p1"
        onSubmitChoice={onSubmit}
      />
    );
    const { rerender } = render(view(first));
    fireEvent.click(screen.getByText("First"));
    fireEvent.click(screen.getByText("Second"));
    fireEvent.click(screen.getByTestId("submit-choice-button"));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledExactlyOnceWith("first"));
    rerender(view(null));
    rerender(
      view({
        ...first,
        nonce: "two",
        context: { subtype: "pay_resources" },
        options: [{ id: "second", kind: "pay", label: "Second" }],
      }),
    );
    await act(async () => {});
    expect(onSubmit).toHaveBeenCalledTimes(1);
  });

  it("offers host undo, redo and restore-after-event controls only when eligible", () => {
    const change = vi.fn();
    const events = [
      {
        id: "event-0",
        timestamp: "00:00",
        version: 1,
        decision_count: 0,
        visibility: "public" as const,
        event: { kind: "game_initialized" as const, round: 1, phase: "strategy", speaker: "p1" },
      },
      {
        id: "event-1",
        timestamp: "00:01",
        version: 2,
        decision_count: 1,
        visibility: "public" as const,
        event: { kind: "decision_resolved" as const },
      },
      {
        id: "event-2",
        timestamp: "00:02",
        version: 3,
        decision_count: 2,
        visibility: "public" as const,
        event: { kind: "decision_resolved" as const },
      },
    ];
    render(
      <GameShell
        header={null}
        board={null}
        playerSheet={null}
        choice={null}
        events={events}
        history={{ cursor: 2, redo_count: 1 }}
        onChangeHistory={change}
        onSubmitChoice={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByTestId("event-log-toggle"));
    expect(screen.queryByRole("button", { name: /^Undo$/ })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Redo one" }));
    fireEvent.click(screen.getByRole("button", { name: /Round 1/ }));
    fireEvent.click(screen.getByRole("button", { name: /Strategy phase/ }));
    fireEvent.click(screen.getByRole("button", { name: "Undo from decision 1" }));
    expect(change.mock.calls).toEqual([["redo"], [{ cursor: 0 }, 2]]);
    expect(screen.getByRole("button", { name: "Undo from decision 2" })).toBeTruthy();
  });
  it("resumes staged production after payment on a fresh legal nonce", async () => {
    const produce = (nonce: string): PendingChoiceDto => ({
      actor: "p1",
      nonce,
      prompt: "produce in 18",
      context: {
        subtype: "produce_unit",
        target: { System: "18" },
        outstanding: [{ amount: 3, paid: 0 }],
      },
      options: [
        {
          id: "build|fighter|1",
          kind: "produce",
          label: "Fighter",
          payload: { unit: "fighter", production_spent: 1, cost: 1, available_resources: 3 },
        },
        { id: "done_producing", kind: "decline", label: "Done" },
      ],
    });
    const payment: PendingChoiceDto = {
      actor: "p1",
      nonce: "pay-2",
      prompt: "Pay 1",
      context: { subtype: "pay_resources" },
      options: [{ id: "trade_good", kind: "pay", label: "Trade good" }],
    };
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const view = (choice: PendingChoiceDto) => (
      <GameShell
        header={<div>Header</div>}
        board={<div>Board</div>}
        playerSheet={<div>Players</div>}
        events={[]}
        choice={choice}
        viewerSeat="p1"
        onSubmitChoice={onSubmit}
      />
    );
    const { rerender } = render(view(produce("produce-1")));
    fireEvent.click(screen.getByTestId("produce-unit-btn-build|fighter|1"));
    fireEvent.click(screen.getByTestId("produce-unit-btn-build|fighter|1"));
    fireEvent.click(screen.getByRole("button", { name: "Confirm builds" }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(onSubmit).toHaveBeenCalledWith("build|fighter|1");
    rerender(view(payment));
    expect(onSubmit).toHaveBeenCalledTimes(1);
    rerender(view(produce("produce-3")));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(2));
    expect(onSubmit).toHaveBeenNthCalledWith(2, "build|fighter|1");
  });
  it("offers system activation directly over the map with activation bar without modal", () => {
    const activation: PendingChoiceDto = {
      actor: "p1",
      nonce: "activation",
      prompt: "Activate a system",
      context: { subtype: "activate_system" },
      options: [{ id: "18", kind: "activate", label: "18", payload: { system: "18" } }],
    };
    render(
      <ChoiceRendererDispatcher
        choice={activation}
        viewerSeat="p1"
        onSubmit={vi.fn()}
        isMinimized={false}
        onMinimizedChange={vi.fn()}
      />,
    );
    expect(screen.queryByTestId("pending-choice-dialog")).not.toBeInTheDocument();
    expect(screen.getByTestId("system-activation-bar")).toBeVisible();
    expect(screen.getByText("Activate a system")).toBeInTheDocument();
  });
  it("presents dynamic choice text and errors from the latest roster without changing submissions", async () => {
    const a = `player_${"a".repeat(64)}`;
    const b = `player_${"b".repeat(64)}`;
    const missing = `player_${"c".repeat(64)}`;
    const lobby: LobbyDto = {
      game_id: "game",
      phase: "running",
      lobby_version: 1,
      host_player_id: a,
      slots: [
        {
          slot_id: "slot_1",
          position: 1,
          occupant: a,
          nickname: "Sam",
          ready: true,
          connected: true,
          can_take_over: false,
        },
        {
          slot_id: "slot_2",
          position: 2,
          occupant: b,
          nickname: "Sam",
          ready: true,
          connected: true,
          can_take_over: false,
        },
      ],
    };
    const dynamic: PendingChoiceDto = {
      actor: a,
      nonce: "dynamic",
      prompt: `${b} offers ${a} a deal`,
      options: [
        {
          id: `deal:${b}`,
          label: `Accept from ${b}`,
          description: `Notify ${missing}; content:${b} stays`,
        },
      ],
    };
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const view = (roster: LobbyDto) => (
      <PlayerIdentityProvider lobby={roster} seatingOrder={[a, b]}>
        <ChoiceRendererDispatcher
          choice={dynamic}
          viewerSeat={a}
          onSubmit={onSubmit}
          lastError={`Unable to notify ${missing}`}
          isMinimized={false}
          onMinimizedChange={vi.fn()}
        />
      </PlayerIdentityProvider>
    );
    const { container, rerender } = render(view(lobby));
    expect(screen.getByTestId("choice-prompt")).toHaveTextContent(
      "Sam (▲ Position 2) offers Sam (● Position 1) a deal",
    );
    expect(screen.getByTestId("choice-error-banner")).toHaveTextContent(
      "Unable to notify Unknown participant",
    );
    expect(container.textContent).not.toContain(missing);
    expect(container.textContent).toContain(`content:${b}`);
    rerender(
      view({
        ...lobby,
        slots: lobby.slots.map((slot) =>
          slot.occupant === b ? { ...slot, nickname: "Renamed" } : slot,
        ),
      }),
    );
    expect(screen.getByTestId("choice-prompt")).toHaveTextContent("Renamed offers Sam a deal");
    await act(async () => {
      fireEvent.click(screen.getByTestId("submit-choice-button"));
    });
    expect(onSubmit).toHaveBeenCalledWith(`deal:${b}`);
    expect(dynamic.prompt).toContain(b);
    expect(dynamic.options[0].label).toContain(b);
  });
  it("owns player and event drawer visibility", () => {
    renderShell();

    const playerDrawer = screen.getByTestId("player-sheet-drawer");
    const eventDrawer = screen.getByLabelText("Event log");
    expect(playerDrawer).not.toHaveClass("app-shell__drawer--open");
    expect(eventDrawer).not.toHaveClass("app-shell__drawer--open");

    fireEvent.click(screen.getByTestId("player-sheet-toggle"));
    expect(playerDrawer).toHaveClass("app-shell__drawer--open");
    expect(screen.getByTestId("player-sheet-toggle")).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByTestId("player-sheet-toggle")).toHaveAttribute(
      "aria-controls",
      "player-sheet-drawer",
    );

    fireEvent.click(screen.getByTestId("event-log-mobile-toggle"));
    expect(eventDrawer).toHaveClass("app-shell__drawer--open");
    expect(playerDrawer).not.toHaveClass("app-shell__drawer--open");
    expect(screen.getByTestId("event-log-mobile-toggle")).toHaveAttribute(
      "aria-controls",
      "event-log-drawer",
    );
    expect(screen.getByTestId("event-log-toggle")).toHaveAttribute("aria-expanded", "true");

    // Escape closes the open mobile drawer
    fireEvent.keyDown(window, { key: "Escape" });
    expect(eventDrawer).not.toHaveClass("app-shell__drawer--open");
  });

  it("renders pending choices through the shell overlay and resets minimization for a new nonce", () => {
    const { rerender } = renderShell(choice);

    expect(screen.getByTestId("pending-choice-dialog")).toHaveClass("choice-dialog");
    fireEvent.click(screen.getByTestId("minimize-choice-button"));
    expect(screen.getByTestId("minimized-choice-banner")).toHaveClass("choice-banner");

    rerender(
      <GameShell
        header={<div>Header</div>}
        board={<div>Board</div>}
        playerSheet={<div>Player sheet</div>}
        events={[]}
        choice={{ ...choice, nonce: "choice-2" }}
        onSubmitChoice={vi.fn().mockResolvedValue(undefined)}
      />,
    );

    expect(screen.getByTestId("pending-choice-dialog")).toBeInTheDocument();
  });

  it("synchronizes selectedOptionId and propagates onSelectOption when options change", () => {
    const onSelectOption = vi.fn();
    render(
      <GameShell
        header={<div>Header</div>}
        board={<div>Board</div>}
        playerSheet={<div>Player sheet</div>}
        events={[]}
        choice={{
          ...choice,
          options: [
            { id: "opt_1", label: "Option 1" },
            { id: "opt_2", label: "Option 2" },
          ],
        }}
        selectedOptionId="opt_2"
        onSelectOption={onSelectOption}
        onSubmitChoice={vi.fn().mockResolvedValue(undefined)}
      />,
    );

    const radio2 = screen.getByDisplayValue("opt_2") as HTMLInputElement;
    expect(radio2.checked).toBe(true);

    const radio1 = screen.getByDisplayValue("opt_1") as HTMLInputElement;
    expect(radio1.checked).toBe(false);

    fireEvent.click(radio1);
    expect(onSelectOption).toHaveBeenCalledWith("opt_1");
  });

  it("dispatches to domain-specific drawers and modals based on workflow subtype", () => {
    // 1. Payment Drawer
    const { rerender } = render(
      <GameShell
        header={<div>Header</div>}
        board={<div>Board</div>}
        playerSheet={<div>Player sheet</div>}
        events={[]}
        choice={{
          actor: "p1",
          nonce: "pay-1",
          prompt: "Pay 3 resources",
          context: { subtype: "pay_resources" },
          options: [{ id: "opt_1", label: "Planet 1" }],
        }}
        viewerSeat="p1"
        onSubmitChoice={vi.fn().mockResolvedValue(undefined)}
      />,
    );
    expect(screen.getByTestId("payment-drawer")).toBeInTheDocument();

    // 2. Combat Resolution Modal
    rerender(
      <GameShell
        header={<div>Header</div>}
        board={<div>Board</div>}
        playerSheet={<div>Player sheet</div>}
        events={[]}
        choice={{
          actor: "p1",
          nonce: "combat-1",
          prompt: "Sustain damage",
          context: { subtype: "sustain_damage" },
          options: [{ id: "opt_1", label: "Dreadnought" }],
        }}
        viewerSeat="p1"
        onSubmitChoice={vi.fn().mockResolvedValue(undefined)}
      />,
    );
    expect(screen.getByTestId("combat-resolution-modal")).toBeInTheDocument();

    // 3. Trade Desk Modal
    rerender(
      <GameShell
        header={<div>Header</div>}
        board={<div>Board</div>}
        playerSheet={<div>Player sheet</div>}
        events={[]}
        choice={{
          actor: "p1",
          nonce: "trade-1",
          prompt: "Propose transaction",
          context: { subtype: "propose_transaction" },
          options: [{ id: "cc1", label: "Swap" }],
        }}
        viewerSeat="p1"
        onSubmitChoice={vi.fn().mockResolvedValue(undefined)}
      />,
    );
    expect(screen.getByTestId("trade-desk-modal")).toBeInTheDocument();

    // 4. Agenda Ballot Modal
    rerender(
      <GameShell
        header={<div>Header</div>}
        board={<div>Board</div>}
        playerSheet={<div>Player sheet</div>}
        events={[]}
        choice={{
          actor: "p1",
          nonce: "agenda-1",
          prompt: "Cast vote",
          context: { subtype: "cast_vote" },
          options: [{ id: "FOR", label: "FOR" }],
        }}
        viewerSeat="p1"
        onSubmitChoice={vi.fn().mockResolvedValue(undefined)}
      />,
    );
    expect(screen.getByTestId("agenda-ballot-modal")).toBeInTheDocument();

    // 5. Reaction Status Bar
    rerender(
      <GameShell
        header={<div>Header</div>}
        board={<div>Board</div>}
        playerSheet={<div>Player sheet</div>}
        events={[]}
        choice={{
          actor: "p1",
          nonce: "reaction-1",
          prompt: "Sabotage?",
          context: { subtype: "play_reaction_when_action_card_played" },
          options: [{ id: "sabotage", label: "Sabotage" }],
        }}
        viewerSeat="p1"
        onSubmitChoice={vi.fn().mockResolvedValue(undefined)}
      />,
    );
    expect(screen.getByTestId("reaction-status-bar")).toBeInTheDocument();

    // 6. Production Builder Drawer
    rerender(
      <GameShell
        header={<div>Header</div>}
        board={<div>Board</div>}
        playerSheet={<div>Player sheet</div>}
        events={[]}
        choice={{
          actor: "p1",
          nonce: "prod-1",
          prompt: "Produce units",
          context: { subtype: "produce_unit" },
          options: [{ id: "produce|fighter", label: "Fighter" }],
        }}
        viewerSeat="p1"
        onSubmitChoice={vi.fn().mockResolvedValue(undefined)}
      />,
    );
    expect(screen.getByTestId("production-builder-drawer")).toBeInTheDocument();

    // 7. Objective scoring uses the dedicated objectives matrix modal.
    rerender(
      <GameShell
        header={<div>Header</div>}
        board={<div>Board</div>}
        playerSheet={<div>Player sheet</div>}
        events={[]}
        choice={{
          actor: "p1",
          nonce: "score-1",
          prompt: "Score an objective",
          context: { subtype: "score_objective" },
          options: [{ id: "obj-1", label: "Objective" }],
        }}
        viewerSeat="p1"
        onSubmitChoice={vi.fn().mockResolvedValue(undefined)}
      />,
    );
    expect(screen.getByTestId("objectives-modal")).toBeInTheDocument();
  });

  it("uses a supplied workflow model when selecting a registered renderer", () => {
    const productionChoice: PendingChoiceDto = {
      actor: "p1",
      nonce: "production-model",
      prompt: "Produce",
      context: { subtype: "produce_unit" },
      options: [{ id: "produce|fighter", label: "Fighter" }],
    };
    const model = deriveChoiceRendererModel(productionChoice, "p1");

    render(
      <ChoiceRendererDispatcher
        choice={choice}
        model={model}
        viewerSeat="p1"
        onSubmit={vi.fn().mockResolvedValue(undefined)}
        isMinimized={false}
        onMinimizedChange={vi.fn()}
      />,
    );

    expect(screen.getByTestId("production-builder-drawer")).toBeInTheDocument();
    expect(screen.queryByTestId("pending-choice-dialog")).not.toBeInTheDocument();
  });
});
