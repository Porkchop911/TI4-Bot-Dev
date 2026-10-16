import { describe, expect, it } from "vitest";
import { NO_ROLLS, actionRolls, logOf, withEvent } from "./rolls";
import { cannonTable, riftRows } from "./select/movementRolls";
import type { GameEvent, PublicRoll } from "./wire";

const event = (id: number, change: Partial<GameEvent>): GameEvent => ({
  id: `g-${id}`,
  timestamp: "",
  visibility: { visibility: "public" },
  event: { kind: "decision_resolved" },
  ...change,
});
/** A decision of the action that starts after `start` decisions. */
const decision = (id: number, start: number) =>
  event(id, {
    decision_count: start + 1,
    action_id: `action_${start + 1}`,
    action_start_cursor: start,
  });
const rolled = (id: number, cursor: number, roll: Partial<PublicRoll>) =>
  event(id, {
    event: { kind: "dice_rolled" },
    decision_count: cursor,
    roll: { index: id, reason: "gravity rift", label: "Gravity rift", faces: [5], ...roll },
  });

describe("the rolls of the action", () => {
  it("are the rolls after the start of the latest action, each once", () => {
    const log = logOf([
      decision(1, 10),
      rolled(2, 11, { index: 0 }),
      decision(3, 20),
      rolled(4, 21, { index: 1 }),
      rolled(5, 22, { index: 2 }),
    ]);
    expect(actionRolls(log).map((roll) => roll.index)).toEqual([1, 2]);
    // A restart tells the same roll again, under another event.
    const again = withEvent(log, rolled(6, 22, { index: 2 }));
    expect(again).toBe(log);
  });

  it("are none before an action started", () => {
    expect(actionRolls(withEvent(NO_ROLLS, rolled(1, 3, {})))).toEqual([]);
  });
});

describe("the dice of a movement", () => {
  const label = (system: string) => `Gravity Rift · #${system}`;

  it("give a row for each rift roll, with what the roll says", () => {
    const rows = riftRows(
      [
        {
          index: 0,
          reason: "gravity rift",
          label: "Gravity rift",
          faces: [5],
          hits_on: 4,
          hits: 1,
          system: "41",
        },
        {
          index: 1,
          reason: "gravity rift",
          label: "Gravity rift",
          faces: [2],
          hits_on: 4,
          hits: 0,
          system: "41",
        },
        {
          index: 2,
          reason: "space cannon",
          label: "Space cannon",
          faces: [9],
          hits_on: 6,
          hits: 1,
        },
      ],
      label,
    );
    expect(rows).toEqual([
      {
        unit: null,
        name: "Gravity Rift · #41",
        text: "Survives on 4+",
        roll: { face: 5, lost: false },
      },
      {
        unit: null,
        name: "Gravity Rift · #41",
        text: "Survives on 4+",
        roll: { face: 2, lost: true },
      },
    ]);
    expect(riftRows([], label)).toBeNull();
  });

  it("give the space cannon offense a side for each seat that fired, the seat of the action first", () => {
    const table = cannonTable(
      [
        {
          index: 0,
          roller: "c",
          reason: "space cannon",
          label: "Space cannon",
          faces: [8, 5],
          hits_on: 6,
          hits: 1,
        },
        {
          index: 1,
          roller: "a",
          reason: "space cannon",
          label: "Space cannon",
          faces: [3],
          hits_on: 6,
          hits: 0,
        },
        {
          index: 2,
          roller: "c",
          reason: "space cannon defense",
          label: "Space cannon defense",
          faces: [9],
          hits_on: 6,
          hits: 1,
        },
      ],
      "a",
      "a",
    );
    expect(table?.score).toBe("1 hit");
    expect(table?.sides.map((side) => [side.seat, side.role, side.isViewer, side.hits])).toEqual([
      ["a", "Attacker", true, 0],
      ["c", "Defender", false, 1],
    ]);
    expect(table?.sides[1].rows[0]).toMatchObject({
      unit: null,
      label: "Space cannon",
      count: null,
      target: 6,
      dice: [
        { roll: 8, hit: true },
        { roll: 5, hit: false },
      ],
      hits: "1 hit",
      scored: true,
    });
    expect(cannonTable([], "a", "a")).toBeNull();
  });
});
