import { describe, expect, it } from "vitest";
import { neighbourProgressDisplay } from "./neighbourProgress.ts";
import type { ParticipantDisplay } from "./playerDisplay.ts";

const display = (id: string): ParticipantDisplay => ({
  label: id.toUpperCase(),
  position: 1,
  color: "#fff",
  symbol: "●",
});

describe("Push Boundaries neighbor display", () => {
  it("orders ours first, theirs second, and takes the standing from the server", () => {
    const shown = neighbourProgressDisplay(
      {
        have: 2,
        threshold: 2,
        satisfied: true,
        neighbours: [
          { player: "b", mine: 12, theirs: 10, ahead: true },
          { player: "c", mine: 12, theirs: 9, ahead: true },
          { player: "d", mine: 12, theirs: 13, ahead: false },
          { player: "e", mine: 12, theirs: 12, ahead: false },
        ],
      },
      display,
    );
    expect(shown?.rows.map((row) => [row.text, row.standing])).toEqual([
      ["12/10", "ahead"],
      ["12/9", "ahead"],
      ["12/13", "behind"],
      ["12/12", "tied"],
    ]);
    expect(shown?.rows[3].aria).toBe("12 planets against E's 12: tied, not ahead");
    expect(shown?.summary).toBe("Ahead of 2 of 4 neighbors · need 2");
  });

  it("is null without neighbor rows", () => {
    expect(neighbourProgressDisplay(undefined, display)).toBeNull();
    expect(neighbourProgressDisplay({ have: 1, threshold: 4, satisfied: false }, display)).toBeNull();
    expect(
      neighbourProgressDisplay({ have: 0, threshold: 2, satisfied: false, neighbours: [] }, display),
    ).toBeNull();
  });

  it("words a single neighbor in the singular", () => {
    expect(
      neighbourProgressDisplay(
        {
          have: 0,
          threshold: 2,
          satisfied: false,
          neighbours: [{ player: "b", mine: 3, theirs: 5, ahead: false }],
        },
        display,
      )?.summary,
    ).toBe("Ahead of 0 of 1 neighbor · need 2");
  });
});
