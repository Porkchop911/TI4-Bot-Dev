import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { GroundCombatResultSummary } from "./GroundCombatResultSummary.tsx";
import type { GroundCombatSummary } from "../presentation/groundCombatSummary.ts";
import { InvasionOverlay } from "./InvasionOverlay.tsx";
import { galleryBoard } from "../dev/galleryBoard.ts";

const summary: GroundCombatSummary = {
  planet: "jord",
  round: 2,
  verdict: { kind: "attacker_won", winner: "a", loser: "d" },
  attacker: {
    seat: "a",
    before: [{ unit: "infantry", count: 3 }],
    after: [{ unit: "infantry", count: 2 }],
    lost: [{ unit: "infantry", count: 1 }],
    sustained: [],
    remaining: 2,
    hits: 3,
  },
  defender: {
    seat: "d",
    before: [{ unit: "infantry", count: 2 }],
    after: [],
    lost: [{ unit: "infantry", count: 2 }],
    sustained: [],
    remaining: 0,
    hits: 1,
  },
};

describe("GroundCombatResultSummary", () => {
  it("names the conqueror, the planet and each side's units before and after", () => {
    render(<GroundCombatResultSummary summary={summary} />);
    expect(screen.getByTestId("ground-result-headline")).toHaveTextContent("conquered jord");
    expect(screen.getByTestId("ground-result-detail")).toHaveTextContent("passes to");
    expect(screen.getByTestId("ground-result-attacker")).toHaveTextContent("Before3 × Infantry");
    expect(screen.getByTestId("ground-result-attacker")).toHaveTextContent("After2 × Infantry");
    expect(screen.getByTestId("ground-result-defender")).toHaveTextContent("Hits scored1");
    expect(screen.getByTestId("ground-result-defender")).toHaveTextContent("AfterNone left");
  });

  it("says the defender held", () => {
    render(<GroundCombatResultSummary summary={{ ...summary, verdict: { kind: "defender_held", winner: "d", loser: "a" } }} />);
    expect(screen.getByTestId("ground-result-headline")).toHaveTextContent("held jord");
    expect(screen.getByTestId("ground-result-detail")).toHaveTextContent("stays with");
  });

  it("says mutual destruction", () => {
    render(<GroundCombatResultSummary summary={{ ...summary, verdict: { kind: "mutual_destruction" } }} />);
    expect(screen.getByTestId("ground-result-headline")).toHaveTextContent("Mutual destruction on jord");
  });

  it("shows in the invasion overlay for a spectator once the last round ended the fight", () => {
    const board = {
      ...galleryBoard,
      invasion: {
        system_id: "18", invasion_seq: 1, invader: "a", phase: "planet_result", planets: ["jord"],
        current_planet: "jord", defender: null, ground_round: 2,
        last_step: {
          planet: "jord", kind: "ground_round", round: 2, harrow_hits: 0, dice: [], hits: { a: 1, d: 1 },
          before: [{ owner: "a", unit_type: "infantry", planet: "jord", damaged: false }, { owner: "d", unit_type: "infantry", planet: "jord", damaged: false }],
          after: [{ owner: "a", unit_type: "infantry", planet: "jord", damaged: false }],
        },
      },
    };
    render(<InvasionOverlay board={board} choice={null} viewerSeat={null} onSubmit={async () => {}} onClose={() => {}} />);
    expect(screen.getByTestId("ground-result-headline")).toHaveTextContent("conquered jord");
  });
});
