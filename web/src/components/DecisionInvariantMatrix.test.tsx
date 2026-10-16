import { act } from "react";
import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ChoiceRendererDispatcher, GameShell } from "./GameShell.tsx";
import { deriveChoiceRendererModel, type ChoiceWorkflowKind } from "../presentation/choiceModel.ts";
import type { PendingChoiceDto, PlayerView } from "../protocol/types.ts";

// These are explicitly synthetic protocol-shaped choices, not captured live sessions.
// Producer references and the boundary each fixture exercises are in the INV-03 plan ledger.
const actor = "seat_a";
const player: PlayerView = {
  id: actor,
  faction: "sol",
  victory_points: 0,
  trade_goods: 2,
  commodities: 0,
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
  action_cards_count: 0,
  secret_objectives_count: 0,
  leaders: {},
};

const cases: Array<{
  name: string;
  choice: PendingChoiceDto;
  workflow: ChoiceWorkflowKind;
  region: string;
  action: string;
  option: string;
  prepare?: string;
}> = [
  {
    name: "empty movement",
    workflow: "tactical_movement",
    region: "tactical-movement-tray",
    action: "commit-moves-btn",
    option: "done_moving",
    choice: {
      actor,
      nonce: "empty-1",
      prompt: "movement",
      context: { subtype: "movement_step", target: { System: "18" } },
      options: [{ id: "done_moving", kind: "decline", label: "finish movement" }],
    },
  },
  {
    name: "normal movement",
    workflow: "tactical_movement",
    region: "tactical-movement-tray",
    prepare: "rally-inc-24-cruiser",
    action: "commit-moves-btn",
    option: "move|24|0",
    choice: {
      actor,
      nonce: "move-1",
      prompt: "movement",
      context: { subtype: "movement_step", target: { System: "18" } },
      options: [
        {
          id: "move|24|0",
          kind: "move",
          label: "move cruiser from 24",
          payload: { origin: "24", unit: "cruiser", capacity: 0 },
        },
        { id: "done_moving", kind: "decline", label: "finish movement" },
      ],
    },
  },
  {
    name: "resource payment",
    workflow: "payment",
    region: "payment-drawer",
    prepare: "planet-card-exhaust|jord",
    action: "confirm-payment-btn",
    option: "exhaust|jord",
    choice: {
      actor,
      nonce: "pay-1",
      prompt: "pay 4 resources",
      context: {
        subtype: "pay_resources",
        outstanding: [{ kind: "resources", amount: 4, paid: 0 }],
      },
      options: [
        {
          id: "exhaust|jord",
          kind: "pay",
          label: "exhaust jord for 4 resources",
          payload: { worth: 4, owed: 4, kind: "resources" },
        },
      ],
    },
  },
  {
    name: "unit production",
    workflow: "production",
    region: "production-builder-drawer",
    action: "done-producing-btn",
    option: "done_producing",
    choice: {
      actor,
      nonce: "produce-1",
      prompt: "produce in 18 (3 left)",
      context: {
        subtype: "produce_unit",
        target: { System: "18" },
        outstanding: [{ kind: "production_capacity", amount: 3, paid: 0 }],
      },
      options: [
        { id: "build|infantry|1", kind: "produce", label: "produce 1x infantry for 1" },
        { id: "done_producing", kind: "decline", label: "produce nothing further" },
      ],
    },
  },
  {
    name: "reaction pass",
    workflow: "action_card_reaction",
    region: "reaction-status-bar",
    action: "pass-reaction-btn",
    option: "decline",
    choice: {
      actor,
      nonce: "react-1",
      prompt: "play an action card (after ACTION_CARD_PLAYED)",
      context: {
        subtype: "play_reaction_after_ACTION_CARD_PLAYED",
        optional: true,
        source: { Reaction: "ACTION_CARD_PLAYED" },
      },
      options: [
        { id: "fs1", kind: "action_card", label: "play Sabotage" },
        { id: "decline", kind: "decline", label: "pass" },
      ],
    },
  },
  {
    name: "constrained generic choice",
    workflow: "generic_selection",
    region: "pending-choice-dialog",
    prepare: "select-generic",
    action: "submit-choice-button",
    option: "card_a",
    choice: {
      actor,
      nonce: "generic-1",
      prompt: "select two cards",
      context: {
        subtype: "synthetic_bounded_selection",
        outstanding: [{ min_selection: 2, max_selection: 2 }],
      },
      options: [
        { id: "card_a", label: "Card A" },
        { id: "card_b", label: "Card B" },
        { id: "card_c", label: "Card C" },
      ],
    },
  },
];

function dispatcher(
  choice: PendingChoiceDto,
  onSubmit: (id: string) => Promise<void>,
  viewerSeat = actor,
) {
  return (
    <ChoiceRendererDispatcher
      choice={choice}
      viewerSeat={viewerSeat}
      players={{ [actor]: player }}
      onSubmit={onSubmit}
      isMinimized={false}
      onMinimizedChange={vi.fn()}
    />
  );
}

describe("INV-03 decision invariant matrix", () => {
  for (const fixture of cases) {
    it(`${fixture.name}: classifies, exposes a keyboard action, and submits only an offered ID`, async () => {
      const { choice, option } = fixture;
      const model = deriveChoiceRendererModel(choice, actor);
      expect(model?.workflow).toBe(fixture.workflow);
      expect(choice.options.map((o) => o.id)).toContain(option);
      expect(deriveChoiceRendererModel(choice, "seat_b")).toBeNull();

      const onSubmit = vi.fn().mockResolvedValue(undefined);
      const { unmount } = render(dispatcher(choice, onSubmit));
      expect(screen.getByTestId(fixture.region)).toBeVisible();
      if (fixture.prepare === "select-generic") {
        expect(screen.getByTestId("submit-choice-button")).toBeDisabled();
        fireEvent.click(screen.getByRole("checkbox", { name: "Card A" }));
        fireEvent.click(screen.getByRole("checkbox", { name: "Card B" }));
        expect(screen.getByRole("checkbox", { name: "Card C" })).toBeDisabled();
      } else if (fixture.prepare?.startsWith("planet-card-")) {
        fireEvent.click(screen.getByTestId(fixture.prepare).querySelector("input")!);
      } else if (fixture.prepare) {
        fireEvent.click(screen.getByTestId(fixture.prepare));
      }
      const button = screen.getByTestId(fixture.action);
      expect(button).toBeVisible();
      expect(button).toBeEnabled();
      expect(button.tagName).toBe("BUTTON");
      await act(async () => {
        fireEvent.click(button);
      });
      await waitFor(() => expect(onSubmit).toHaveBeenNthCalledWith(1, option));
      expect(onSubmit.mock.calls.every(([id]) => choice.options.some((o) => o.id === id))).toBe(
        true,
      );
      unmount();

      const otherSubmit = vi.fn().mockResolvedValue(undefined);
      const other = render(dispatcher(choice, otherSubmit, "seat_b"));
      expect(screen.queryByTestId(fixture.region)).not.toBeInTheDocument();
      expect(screen.queryByTestId("pending-choice-dialog")).not.toBeInTheDocument();
      expect(otherSubmit).not.toHaveBeenCalled();
      other.unmount();
    });
  }

  it("normal movement waits for the acknowledged choice to change nonce before finishing", async () => {
    const first = cases[1].choice;
    const finish: PendingChoiceDto = { ...first, nonce: "move-2", options: [first.options[1]] };
    let acknowledge!: () => void;
    const onSubmit = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            acknowledge = resolve;
          }),
      )
      .mockResolvedValue(undefined);
    const { rerender } = render(dispatcher(first, onSubmit));
    fireEvent.click(screen.getByTestId("rally-inc-24-cruiser"));
    fireEvent.click(screen.getByTestId("commit-moves-btn"));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledExactlyOnceWith("move|24|0"));
    await act(async () => {
      acknowledge();
    });
    expect(onSubmit).toHaveBeenCalledTimes(1);
    rerender(dispatcher(finish, onSubmit));
    await waitFor(() => expect(onSubmit).toHaveBeenNthCalledWith(2, "done_moving"));
    expect(finish.options.map((o) => o.id)).toContain(onSubmit.mock.calls[1][0]);
  });

  it("does not expose an actor model supplied to the dispatcher to another seat", () => {
    const choice = cases[0].choice;
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(
      <ChoiceRendererDispatcher
        choice={choice}
        model={deriveChoiceRendererModel(choice, actor)}
        viewerSeat="seat_b"
        onSubmit={onSubmit}
        isMinimized
        onMinimizedChange={vi.fn()}
      />,
    );
    expect(screen.queryByTestId("choice-minimized-pill")).not.toBeInTheDocument();
    expect(screen.queryByTestId("pending-choice-dialog")).not.toBeInTheDocument();
    expect(onSubmit).not.toHaveBeenCalled();
  });

  const modalCases: Array<{
    subtype: string;
    options: PendingChoiceDto["options"];
    region: string;
  }> = [
    {
      subtype: "movement_step",
      options: [{ id: "done_moving", kind: "decline", label: "Done" }],
      region: "decision-modal",
    },
    {
      subtype: "load_cargo",
      options: [{ id: "done_loading", kind: "decline", label: "Done" }],
      region: "decision-modal",
    },
    {
      subtype: "pay_resources",
      options: [{ id: "trade_good", kind: "pay", label: "Spend TG", payload: { worth: 1 } }],
      region: "decision-modal",
    },
    {
      subtype: "pay_influence",
      options: [{ id: "exhaust|jord", kind: "pay", label: "Jord", payload: { worth: 2 } }],
      region: "decision-modal",
    },
    {
      subtype: "produce_unit",
      options: [{ id: "done_producing", kind: "decline", label: "Done" }],
      region: "production-builder-drawer",
    },
    {
      subtype: "place_unit",
      options: [{ id: "space", kind: "place", label: "Space" }],
      region: "production-builder-drawer",
    },
    {
      subtype: "sustain_damage",
      options: [{ id: "decline", kind: "decline", label: "Pass" }],
      region: "combat-resolution-modal",
    },
    {
      subtype: "assign_casualty",
      options: [{ id: "destroy|fighter", kind: "casualty", label: "Fighter" }],
      region: "combat-resolution-modal",
    },
    {
      subtype: "retreat_to",
      options: [{ id: "retreat|18", kind: "retreat", label: "System 18" }],
      region: "combat-resolution-modal",
    },
    {
      subtype: "propose_transaction",
      options: [{ id: "cc1", kind: "offer", label: "Swap" }],
      region: "trade-desk-modal",
    },
    {
      subtype: "answer_transaction",
      options: [{ id: "refuse", kind: "decline", label: "Refuse" }],
      region: "trade-desk-modal",
    },
    {
      subtype: "cast_vote",
      options: [{ id: "FOR", kind: "outcome", label: "For" }],
      region: "agenda-ballot-modal",
    },
    {
      subtype: "vote_exhaust_planet",
      options: [{ id: "decline", kind: "decline", label: "Done" }],
      region: "agenda-ballot-modal",
    },
    {
      subtype: "play_reaction_after_ACTION_CARD_PLAYED",
      options: [{ id: "decline", kind: "decline", label: "Pass" }],
      region: "decision-modal",
    },
    {
      subtype: "score_objective",
      options: [{ id: "objective", label: "Score" }],
      region: "objectives-modal",
    },
    {
      subtype: "other_choice",
      options: [{ id: "other", label: "Other" }],
      region: "pending-choice-dialog",
    },
  ];

  for (const { subtype, options, region } of modalCases) {
    it(`${subtype}: opens as a modal, minimizes to inspect the board, and resumes`, () => {
      const choice: PendingChoiceDto = {
        actor,
        nonce: subtype,
        prompt: `Decide ${subtype}`,
        context: { subtype },
        options,
      };
      render(
        <GameShell
          header={<div>Header</div>}
          board={<button type="button">Inspect board</button>}
          playerSheet={<div>Players</div>}
          events={[]}
          choice={choice}
          viewerSeat={actor}
          players={{ [actor]: player }}
          onSubmitChoice={vi.fn().mockResolvedValue(undefined)}
        />,
      );
      expect(screen.getByTestId(region).closest('[role="dialog"]')).not.toBeNull();
      const minimize = screen
        .getByTestId(region)
        .closest('[role="dialog"]')!
        .querySelector(
          'button[aria-label^="Close"], button[aria-label^="Minimize"], [data-testid="minimize-choice-button"]',
        )!;
      expect(minimize).not.toBeNull();
      fireEvent.click(minimize!);
      const minimizedRegion = screen.queryByTestId(region);
      if (minimizedRegion) expect(minimizedRegion).not.toBeVisible();
      expect(screen.getByRole("button", { name: "Inspect board" })).toBeEnabled();
      const resume = screen.getByRole("button", { name: /resume decision|open decision/i });
      fireEvent.click(resume);
      expect(screen.getByTestId(region).closest('[role="dialog"]')).not.toBeNull();
    });
  }

  it("activate_system: presents on the map with system-activation-bar and never traps in a modal dialog", () => {
    const choice: PendingChoiceDto = {
      actor,
      nonce: "activate",
      prompt: "Decide activate_system",
      context: { subtype: "activate_system" },
      options: [{ id: "18", kind: "activate", label: "System 18", payload: { system: "18" } }],
    };
    render(
      <GameShell
        header={<div>Header</div>}
        board={<button type="button">Inspect board</button>}
        playerSheet={<div>Players</div>}
        events={[]}
        choice={choice}
        viewerSeat={actor}
        players={{ [actor]: player }}
        onSubmitChoice={vi.fn().mockResolvedValue(undefined)}
      />,
    );
    expect(screen.queryByTestId("pending-choice-dialog")).not.toBeInTheDocument();
    expect(screen.getByTestId("system-activation-bar")).toBeVisible();
    expect(screen.getByText("Decide activate_system")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Inspect board" })).toBeEnabled();
  });

  it("submits a mixed resource payment once per new offered nonce and keeps the draft on minimize", async () => {
    const pay = (
      nonce: string,
      paid: number,
      options: PendingChoiceDto["options"],
    ): PendingChoiceDto => ({
      actor,
      nonce,
      prompt: "Pay 4 resources",
      context: { subtype: "pay_resources", outstanding: [{ kind: "resources", amount: 4, paid }] },
      options,
    });
    const planet = {
      id: "exhaust|arinam",
      kind: "pay",
      label: "Arinam",
      payload: { worth: 2, planet_name: "Arinam" },
    };
    const tg = { id: "trade_good", kind: "pay", label: "Trade good", payload: { worth: 1 } };
    const initial = pay("pay-first", 0, [planet, tg]);
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const view = (choice: PendingChoiceDto) => (
      <GameShell
        header={<div>Header</div>}
        board={<button type="button">Inspect board</button>}
        playerSheet={<div>Players</div>}
        events={[]}
        choice={choice}
        viewerSeat={actor}
        players={{ [actor]: player }}
        onSubmitChoice={onSubmit}
      />
    );
    const { rerender } = render(view(initial));
    fireEvent.click(screen.getByTestId("planet-card-exhaust|arinam").querySelector("input")!);
    fireEvent.click(screen.getByTestId("tg-increment-btn"));
    fireEvent.click(screen.getByTestId("tg-increment-btn"));
    fireEvent.click(screen.getByRole("button", { name: "Minimize decision" }));
    expect(screen.getByRole("button", { name: "Inspect board" })).toBeEnabled();
    fireEvent.click(screen.getByTestId("resume-decision-btn"));
    expect(screen.getByTestId("committed-amount").querySelector("[aria-label]")).toHaveAttribute("aria-label", "4 resources");
    fireEvent.click(screen.getByTestId("confirm-payment-btn"));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledExactlyOnceWith(planet.id));
    rerender(view(pay("pay-second", 2, [tg])));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(2));
    expect(onSubmit).toHaveBeenNthCalledWith(2, tg.id);
    rerender(view(pay("pay-third", 3, [tg])));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(3));
    expect(onSubmit).toHaveBeenNthCalledWith(3, tg.id);
  });

  it("stops a payment draft when the next decision is an intervening reaction", async () => {
    const payment: PendingChoiceDto = {
      actor,
      nonce: "pay-interrupted",
      prompt: "Pay 2 resources",
      context: {
        subtype: "pay_resources",
        outstanding: [{ kind: "resources", amount: 2, paid: 0 }],
      },
      options: [{ id: "trade_good", kind: "pay", label: "Trade good", payload: { worth: 1 } }],
    };
    const reaction: PendingChoiceDto = {
      actor,
      nonce: "reaction-interrupted",
      prompt: "React",
      context: { subtype: "play_reaction_after_ACTION_CARD_PLAYED" },
      options: [{ id: "decline", kind: "decline", label: "Pass" }],
    };
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const view = (choice: PendingChoiceDto) => (
      <GameShell
        header={<div>Header</div>}
        board={<div>Board</div>}
        playerSheet={<div>Players</div>}
        events={[]}
        choice={choice}
        viewerSeat={actor}
        players={{ [actor]: player }}
        onSubmitChoice={onSubmit}
      />
    );
    const { rerender } = render(view(payment));
    fireEvent.click(screen.getByTestId("tg-increment-btn"));
    fireEvent.click(screen.getByTestId("tg-increment-btn"));
    fireEvent.click(screen.getByTestId("confirm-payment-btn"));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledExactlyOnceWith("trade_good"));
    rerender(view(reaction));
    expect(screen.getByTestId("reaction-status-bar")).toBeVisible();
    expect(onSubmit).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByTestId("pass-reaction-btn"));
    await waitFor(() => expect(onSubmit).toHaveBeenNthCalledWith(2, "decline"));
  });
});
