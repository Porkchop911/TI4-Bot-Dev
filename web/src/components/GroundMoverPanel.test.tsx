import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { GroundMoverPanel } from "./GroundMoverPanel.tsx";
import type { GroundMoverView } from "../presentation/groundMover.ts";

const view: GroundMoverView = {
  doneId: "decline",
  planets: [
    { id: "mellon", name: "Mellon", forces: "3 Infantry" },
    { id: "zohbat", name: "Zohbat", forces: "1 Infantry" },
  ],
  moves: [
    {
      optionId: "move|mellon|zohbat|infantry",
      from: "mellon",
      fromName: "Mellon",
      to: "zohbat",
      toName: "Zohbat",
      unit: "infantry",
      unitName: "Infantry",
      count: 3,
      fromBefore: 3,
      fromAfter: 0,
      toBefore: 1,
      toAfter: 4,
    },
  ],
};

describe("GroundMoverPanel", () => {
  it("shows the planets and each move with counts before and after", () => {
    render(<GroundMoverPanel view={view} onChoose={() => {}} />);
    expect(screen.getAllByTestId("mover-planet")).toHaveLength(2);
    const move = screen.getByTestId("mover-move");
    expect(move).toHaveTextContent("3 Infantry");
    expect(move).toHaveTextContent("3 → 0");
    expect(move).toHaveTextContent("1 → 4");
    expect(move).toHaveAccessibleName(/Move 3 Infantry from Mellon to Zohbat: Mellon 3 to 0, Zohbat 1 to 4/);
  });

  it("answers with the move or with Done", () => {
    const onChoose = vi.fn();
    render(<GroundMoverPanel view={view} onChoose={onChoose} />);
    fireEvent.click(screen.getByTestId("mover-move"));
    fireEvent.click(screen.getByTestId("mover-done"));
    expect(onChoose.mock.calls).toEqual([["move|mellon|zohbat|infantry"], ["decline"]]);
  });

  it("does nothing while an answer is on its way", () => {
    render(<GroundMoverPanel view={view} disabled onChoose={() => {}} />);
    expect(screen.getByTestId("mover-move")).toBeDisabled();
    expect(screen.getByTestId("mover-done")).toBeDisabled();
  });
});
