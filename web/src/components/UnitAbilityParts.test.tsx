import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/react";
import { PendingChoiceModal } from "./PendingChoiceModal.tsx";
import type { PendingChoiceDto } from "../protocol/types.ts";

const choice: PendingChoiceDto = {
  prompt: "Orbital Drop: deploy a mech on jord",
  actor: "a",
  nonce: "n2",
  context: { subtype: "orbital_drop_deploy_mech", optional: true },
  options: [
    {
      id: "deploy|sol_mech|1",
      label: "deploy 1 sol_mech for 3 resources",
      kind: "produce",
      payload: { unit: "sol_mech", count: 1, cost: 3, system: "18", planet: "jord", orbital_drop_deploy: true },
    },
    { id: "decline", label: "Pass", kind: "decline" },
  ],
};

describe("unit ability options", () => {
  it("shows names and cost, and keeps both rows and the submit control", () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<PendingChoiceModal choice={choice} onSubmit={onSubmit} />);
    const rows = screen.getAllByTestId("choice-option");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveAttribute("data-option-id", "deploy|sol_mech|1");
    expect(rows[0]).toHaveTextContent("Deploy Mech");
    expect(rows[0]).toHaveTextContent("Cost: 3");
    expect(within(rows[0]).getByLabelText("3 resources")).toBeInTheDocument();
    expect(rows[0]).not.toHaveTextContent("sol_mech");
    fireEvent.click(rows[0].querySelector("input")!);
    fireEvent.click(screen.getByTestId("submit-choice-button"));
    expect(onSubmit).toHaveBeenCalledWith("deploy|sol_mech|1");
  });
});
