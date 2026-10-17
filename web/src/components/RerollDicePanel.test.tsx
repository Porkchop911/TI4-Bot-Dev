import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { RerollDicePanel } from "./RerollDicePanel.tsx";
import { describeRerollDice } from "../presentation/rerollDice.ts";

const view = describeRerollDice({
  context: { subtype: "fire_team_reroll", source: { ActionCard: "fire_team" } } as never,
  options: [{ id: "reroll|0:0", kind: "reroll_die", label: "", payload: {} }],
  details: {
    current: 1,
    dice: [
      { unit: "infantry", unit_index: 0, die: 0, face: 3, bonus: 0, hits_on: 8, rerolled: false },
      { unit: "infantry", unit_index: 0, die: 1, face: 9, bonus: 0, hits_on: 8, rerolled: false },
    ],
  },
})!;

describe("RerollDicePanel", () => {
  it("shows each die with its face and verdict, and what is already decided", () => {
    render(<RerollDicePanel view={view} onConfirm={() => {}} />);
    const dice = screen.getAllByTestId("reroll-die");
    expect(dice).toHaveLength(2);
    expect(dice[1]).toHaveAttribute("data-hit", "true");
    expect(dice[0]).toBeDisabled(); // its question was answered before this one
    expect(dice[1]).toHaveAccessibleName(/shows 9, needs 8 or more, hit, kept/);
    expect(screen.getByText(/hits on 8\+/)).toBeInTheDocument();
  });

  it("ticks a die by click and by keyboard, and says what that is worth", () => {
    const onConfirm = vi.fn();
    render(<RerollDicePanel view={view} onConfirm={onConfirm} />);
    const die = screen.getAllByTestId("reroll-die")[1];
    expect(screen.getByTestId("reroll-confirm")).toHaveTextContent("Keep all dice");
    fireEvent.click(die);
    expect(die).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("reroll-summary")).toHaveTextContent("Rerolling 1 die");
    expect(screen.getByTestId("reroll-confirm")).toHaveTextContent("Reroll 1 die");
    fireEvent.click(screen.getByTestId("reroll-confirm"));
    expect([...onConfirm.mock.calls[0][0]]).toEqual([1]);
    fireEvent.click(die);
    expect(die).toHaveAttribute("aria-pressed", "false");
  });

  it("is inert while an answer is being sent", () => {
    render(<RerollDicePanel view={view} disabled onConfirm={() => {}} />);
    expect(screen.getByTestId("reroll-confirm")).toBeDisabled();
    for (const die of screen.getAllByTestId("reroll-die")) expect(die).toBeDisabled();
  });
});
