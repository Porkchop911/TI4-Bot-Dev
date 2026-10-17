import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { NumberStepperPanel } from "./NumberStepperPanel.tsx";
import { describeNumberStepper } from "../presentation/numberStepper.ts";

const view = describeNumberStepper({
  context: { subtype: "bribery_pick_count", source: { ActionCard: "bribery" } } as never,
  options: [0, 1, 2, 3].map((i) => ({ id: String(i), kind: "count", label: `spend ${i}`, payload: {} })),
})!;

describe("NumberStepperPanel", () => {
  it("starts at nothing and steps within the range", () => {
    render(<NumberStepperPanel view={view} onConfirm={() => {}} />);
    expect(screen.getByTestId("stepper-input")).toHaveValue(0);
    expect(screen.getByTestId("stepper-minus")).toBeDisabled();
    fireEvent.click(screen.getByTestId("stepper-plus"));
    fireEvent.click(screen.getByTestId("stepper-plus"));
    expect(screen.getByTestId("stepper-input")).toHaveValue(2);
    expect(screen.getByTestId("stepper-summary")).toHaveTextContent("Spend 2 trade goods: cast 2 additional votes");
    fireEvent.click(screen.getByTestId("stepper-max"));
    expect(screen.getByTestId("stepper-input")).toHaveValue(3);
    expect(screen.getByTestId("stepper-plus")).toBeDisabled();
  });

  it("clamps typed numbers to what is allowed", () => {
    render(<NumberStepperPanel view={view} onConfirm={() => {}} />);
    fireEvent.change(screen.getByTestId("stepper-input"), { target: { value: "9" } });
    expect(screen.getByTestId("stepper-input")).toHaveValue(3);
  });

  it("confirms the engine option for the chosen amount", () => {
    const onConfirm = vi.fn();
    render(<NumberStepperPanel view={view} onConfirm={onConfirm} />);
    expect(screen.getByTestId("stepper-confirm")).toHaveTextContent("Spend nothing");
    fireEvent.click(screen.getByTestId("stepper-plus"));
    expect(screen.getByTestId("stepper-confirm")).toHaveTextContent("Spend 1");
    fireEvent.click(screen.getByTestId("stepper-confirm"));
    expect(onConfirm).toHaveBeenCalledWith("1");
  });

  it("is inert while an answer is being sent", () => {
    render(<NumberStepperPanel view={view} disabled onConfirm={() => {}} />);
    expect(screen.getByTestId("stepper-confirm")).toBeDisabled();
    expect(screen.getByTestId("stepper-plus")).toBeDisabled();
  });
});
