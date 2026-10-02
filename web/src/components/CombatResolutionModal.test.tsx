import { act } from "react";
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { CombatResolutionModal } from "./CombatResolutionModal.tsx";
import { PendingChoiceDto } from "../protocol/types.ts";

describe("CombatResolutionModal", () => {
  it("renders sustain damage stage with Direct Hit warning and handles sustain submit", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    const sustainChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "10",
      prompt: "Sustain damage on a ship",
      context: {
        subtype: "sustain_damage",
      },
      options: [
        { id: "sustain:dreadnought:1", label: "Dreadnought (System 18)", kind: "sustain" },
        { id: "decline", label: "Do not sustain damage", kind: "decline" },
      ],
    };

    render(
      <CombatResolutionModal
        isOpen={true}
        choice={sustainChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    expect(screen.getByTestId("combat-stage-title")).toHaveTextContent("Space Combat");
    expect(screen.queryByText(/Caution: Opponents holding "Direct Hit"/i)).not.toBeInTheDocument();

    const sustainBtn = screen.getByTestId("sustain-opt-sustain:dreadnought:1");
    expect(sustainBtn).toBeInTheDocument();
    await act(async () => {
      fireEvent.click(sustainBtn);
    });

    expect(onSubmit).toHaveBeenCalledWith("sustain:dreadnought:1");
  });

  it("handles decline option in sustain damage stage", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    const sustainChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "11",
      prompt: "Sustain damage",
      context: {
        subtype: "sustain_damage",
      },
      options: [
        { id: "sustain:dreadnought:1", label: "Dreadnought", kind: "sustain" },
        { id: "decline", label: "Decline", kind: "decline" },
      ],
    };

    render(
      <CombatResolutionModal
        isOpen={true}
        choice={sustainChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    const declineBtn = screen.getByTestId("decline-sustain-btn");
    await act(async () => {
      fireEvent.click(declineBtn);
    });
    expect(onSubmit).toHaveBeenCalledWith("decline");
  });

  it("assigns exactly one offered casualty per hit despite multiple hits remaining", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    const casualtyChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "12",
      prompt: "Assign 2 hits",
      context: {
        subtype: "assign_casualty",
        outstanding: [{ amount: 2 }],
      },
      options: [
        {
          id: "destroy|fighter",
          label: "destroy fighter (system 18)",
          kind: "casualty",
          payload: { unit: "fighter" },
        },
        {
          id: "destroy|cruiser|damaged",
          label: "destroy cruiser (system 18)",
          kind: "casualty",
          payload: { unit: "cruiser", damaged: true },
        },
      ],
    };

    render(
      <CombatResolutionModal
        isOpen={true}
        choice={casualtyChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    expect(screen.getByTestId("combat-stage-title")).toHaveTextContent("Space Combat");
    expect(screen.queryByText(/2 hits remaining/)).not.toBeInTheDocument();
    expect(screen.queryByTestId("confirm-casualties-btn")).not.toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /destroy/i })).toHaveLength(2);
    await act(async () => {
      fireEvent.click(screen.getByTestId("casualty-opt-destroy|fighter"));
    });
    expect(onSubmit).toHaveBeenCalledExactlyOnceWith("destroy|fighter");
  });

  it("does not batch or invent an auto-cheapest sequence from deduplicated options", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    const casualtyChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "13",
      prompt: "Assign 2 hits",
      context: {
        subtype: "assign_casualty",
        outstanding: [{ amount: 2 }],
      },
      options: [
        {
          id: "destroy|cruiser|0",
          label: "destroy cruiser",
          kind: "casualty",
          payload: { unit: "cruiser" },
        },
        {
          id: "destroy|fighter|0",
          label: "destroy fighter",
          kind: "casualty",
          payload: { unit: "fighter" },
        },
        {
          id: "destroy|fighter|1",
          label: "destroy fighter",
          kind: "casualty",
          payload: { unit: "fighter" },
        },
      ],
    };

    render(
      <CombatResolutionModal
        isOpen={true}
        choice={casualtyChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    expect(screen.queryByTestId("auto-cheapest-btn")).not.toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /destroy/i })).toHaveLength(3);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("renders retreat stage and handles option selection", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    const retreatChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "14",
      prompt: "Announce retreat",
      context: {
        subtype: "announce_retreat",
      },
      options: [
        { id: "announce_retreat:yes", label: "Announce Retreat" },
        { id: "announce_retreat:no", label: "Decline Retreat" },
      ],
    };

    render(
      <CombatResolutionModal
        isOpen={true}
        choice={retreatChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    expect(screen.getByTestId("combat-stage-title")).toHaveTextContent("Space Combat");
    const optBtn = screen.getByTestId("retreat-opt-announce_retreat:yes");
    await act(async () => {
      fireEvent.click(optBtn);
    });
    expect(onSubmit).toHaveBeenCalledWith("announce_retreat:yes");
  });

  it("displays spectator notice when viewerSeat is not the active actor", () => {
    const onSubmit = vi.fn();
    const onClose = vi.fn();

    const sustainChoice: PendingChoiceDto = {
      actor: "seat_2",
      nonce: "15",
      prompt: "Sustain damage",
      context: {
        subtype: "sustain_damage",
      },
      options: [{ id: "sustain:carrier:1", label: "Carrier" }],
    };

    render(
      <CombatResolutionModal
        isOpen={true}
        choice={sustainChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    expect(screen.getByTestId("spectator-combat-notice")).toHaveTextContent(
      "Observing combat resolution in progress for Unknown participant...",
    );
  });

  it("displays dice feed and error alert when provided", () => {
    const onSubmit = vi.fn();
    const onClose = vi.fn();

    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "16",
      prompt: "Sustain damage",
      context: {
        subtype: "sustain_damage",
      },
      options: [{ id: "decline", label: "Decline" }],
    };

    const diceRolls = [
      { unit: "Cruiser", roll: 8, target: 7, hit: true },
      { unit: "Fighter", roll: 4, target: 9, hit: false },
    ];

    render(
      <CombatResolutionModal
        isOpen={true}
        choice={choice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
        lastError="Invalid selection"
        recentDiceRolls={diceRolls}
      />,
    );

    expect(screen.getByTestId("combat-roll-group-seat_1-cruiser")).toHaveTextContent("1 hit");
    expect(screen.getByTestId("combat-roll-group-seat_1-fighter")).toHaveTextContent("0 hits");
    expect(screen.getByTestId("combat-error-banner")).toHaveTextContent("Invalid selection");
  });
});
