import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { CombatResultSummary } from "./CombatResultSummary.tsx";
import type { CombatSummary } from "../presentation/combatSummary.ts";

const summary: CombatSummary = {
  round: 2,
  verdict: { kind: "won", winner: "seat_1", loser: "seat_2" },
  attacker: {
    seat: "seat_1",
    lost: [{ unit: "fighter", count: 2 }],
    sustained: [{ unit: "dreadnought", count: 1 }],
    remaining: 10,
    hits: 4,
  },
  defender: { seat: "seat_2", lost: [{ unit: "cruiser", count: 2 }], sustained: [], remaining: 0, hits: 2 },
};

describe("CombatResultSummary", () => {
  it("names the winner and lists losses and sustained damage per side", () => {
    render(<CombatResultSummary summary={summary} />);
    expect(screen.getByTestId("combat-result-headline")).toHaveTextContent("won the battle");
    const attacker = screen.getByTestId("combat-result-attacker");
    expect(attacker).toHaveTextContent("2 × Fighters");
    expect(attacker).toHaveTextContent("1 × Dreadnought");
    expect(attacker).toHaveTextContent("Hits scored4");
    const defender = screen.getByTestId("combat-result-defender");
    expect(defender).toHaveTextContent("2 × Cruisers");
    expect(defender).toHaveTextContent("Ships left0");
    expect(defender).not.toHaveTextContent("Sustained damage");
  });

  it("says no ships were lost when a side lost none", () => {
    render(
      <CombatResultSummary
        summary={{ ...summary, verdict: { kind: "both_remain" }, defender: { ...summary.defender, lost: [], remaining: 3 } }}
      />,
    );
    expect(screen.getByTestId("combat-result-headline")).toHaveTextContent("Both fleets survive");
    expect(screen.getByTestId("combat-result-defender")).toHaveTextContent("No ships");
  });
});
