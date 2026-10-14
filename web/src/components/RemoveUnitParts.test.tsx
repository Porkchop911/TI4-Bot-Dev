import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { PendingChoiceModal } from "./PendingChoiceModal.tsx";
import type { BoardView, PendingChoiceDto } from "../protocol/types.ts";

const board = {
  systems: {
    "14": {
      system_id: "14",
      command_tokens: [],
      planets: {},
      units: [
        { unit_type: "carrier", owner: "a", planet: null, damaged: false },
        { unit_type: "fighter", owner: "a", planet: null, damaged: false },
      ],
    },
  },
} as unknown as BoardView;

const choice: PendingChoiceDto = {
  prompt: "remove a unit: over fleet supply in 14",
  actor: "a",
  nonce: "n1",
  details: { fleet_limit: 1, fleet_charged: 2 },
  options: [{ id: "remove|0", label: "remove sol_carrier", kind: "remove" }],
};

describe("remove a unit", () => {
  it("names the unit and the supply, and the option row still submits", () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<PendingChoiceModal choice={choice} boardView={board} onSubmit={onSubmit} />);
    expect(screen.getByTestId("remove-unit-headline")).toHaveTextContent(
      "1 over the fleet supply of 1",
    );
    const row = screen.getByTestId("choice-option");
    expect(row).toHaveAttribute("data-option-id", "remove|0");
    expect(row).toHaveTextContent("Remove Carrier");
    expect(row).not.toHaveTextContent("sol_carrier");
    expect(screen.getByTestId("remove-unit-note")).toHaveTextContent("1 Fighter");
    fireEvent.click(screen.getByRole("radio"));
    fireEvent.click(screen.getByTestId("submit-choice-button"));
    expect(onSubmit).toHaveBeenCalledWith("remove|0");
  });

  it("still lists the option without a board", () => {
    render(<PendingChoiceModal choice={choice} onSubmit={vi.fn()} />);
    expect(screen.getByTestId("choice-option")).toHaveTextContent("Remove Carrier");
  });
});
