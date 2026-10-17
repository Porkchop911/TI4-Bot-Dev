import { describe, expect, it } from "vitest";
import { describeGroundMover, isGroundMover } from "./groundMover.ts";
import type { BoardView } from "../protocol/types.ts";

const board = {
  systems: {
    "30": {
      system_id: "30",
      command_tokens: [],
      planets: {},
      units: [
        { unit_type: "infantry", owner: "me", planet: "mellon", damaged: false },
        { unit_type: "infantry", owner: "me", planet: "mellon", damaged: false },
        { unit_type: "infantry", owner: "me", planet: "mellon", damaged: false },
        { unit_type: "mech", owner: "me", planet: "mellon", damaged: false },
        { unit_type: "infantry", owner: "me", planet: "zohbat", damaged: false },
        { unit_type: "infantry", owner: "rival", planet: "zohbat", damaged: false },
      ],
    },
  },
} as unknown as BoardView;

const choice = {
  actor: "me",
  context: { subtype: "ghost_squad_move" } as never,
  options: [
    { id: "move|mellon|zohbat|infantry", kind: "move", label: "move 3 infantry from mellon to zohbat", payload: { from: "mellon", to: "zohbat", unit: "infantry", count: 3 } },
    { id: "move|mellon|zohbat|mech", kind: "move", label: "move 1 mech from mellon to zohbat", payload: {} },
    { id: "move|zohbat|mellon|infantry", kind: "move", label: "move 1 infantry from zohbat to mellon", payload: {} },
    { id: "decline", kind: "decline", label: "Decline", payload: {} },
  ],
};

describe("Ghost Squad's ground-force mover", () => {
  it("recognises the card's question", () => {
    expect(isGroundMover(choice)).toBe(true);
    expect(isGroundMover({ context: { subtype: "spy_pick_player" } as never })).toBe(false);
  });

  it("lists each move with the stack size and the counts before and after", () => {
    const view = describeGroundMover(choice, board)!;
    expect(view.moves).toHaveLength(3);
    const first = view.moves[0];
    expect(first).toMatchObject({ unitName: "Infantry", count: 3, fromBefore: 3, fromAfter: 0, toBefore: 1, toAfter: 4 });
    // Without a payload the id and label still say it (older engines).
    expect(view.moves[1]).toMatchObject({ from: "mellon", to: "zohbat", unit: "mech", count: 1, fromBefore: 1, toBefore: 0, toAfter: 1 });
    expect(view.doneId).toBe("decline");
  });

  it("shows only your own forces on each planet", () => {
    const view = describeGroundMover(choice, board)!;
    const zohbat = view.planets.find((planet) => planet.id === "zohbat");
    expect(zohbat?.forces).toBe("1 Infantry");
    expect(view.planets.find((planet) => planet.id === "mellon")?.forces).toBe("3 Infantry, 1 Mech");
  });

  it("is nothing for another decision or an option it cannot read", () => {
    expect(describeGroundMover({ ...choice, context: { subtype: "other" } as never }, board)).toBeNull();
    expect(describeGroundMover({ ...choice, options: [{ id: "weird", kind: "move", label: "?", payload: {} }] }, board)).toBeNull();
  });
});
