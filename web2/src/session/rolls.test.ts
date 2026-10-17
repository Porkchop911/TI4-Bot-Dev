import { describe, expect, it } from "vitest";
import { NO_ROLLS, actionRolls, logOf, withEvent } from "./rolls";
import {
  type ShipsOf,
  cannonPreview,
  cannonTable,
  removedUnits,
  riftRows,
} from "./select/movementRolls";
import type { Catalog, GameEvent, PublicRoll } from "./wire";

/** The unit cards that the rolls name, as the catalogue prints them. */
const printed = {
  records: {
    units: [
      { id: "cruiser", base_type: "cruiser", name: "Cruiser I" },
      { id: "cruiser2", base_type: "cruiser", name: "Cruiser II" },
      { id: "carrier", base_type: "carrier", name: "Carrier I" },
      { id: "fighter", base_type: "fighter", name: "Fighter I" },
      { id: "infantry", base_type: "infantry", name: "Infantry I" },
      { id: "infantry2", base_type: "infantry", name: "Infantry II" },
      { id: "pds", base_type: "pds", name: "PDS I" },
      { id: "pds2", base_type: "pds", name: "PDS II" },
    ],
  },
} as unknown as Catalog;

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
  const rift = (index: number, face: number, change: Partial<PublicRoll> = {}): PublicRoll => ({
    index,
    roller: "a",
    reason: "gravity rift",
    label: "Gravity rift",
    faces: [face],
    hits_on: 4,
    hits: face >= 4 ? 1 : 0,
    system: "41",
    ...change,
  });

  it("give a row for each ship that leaves a rift, with where it came from and its cargo", () => {
    const rows = riftRows(
      [
        rift(0, 5, { subject: { unit: "cruiser2", origin: "41" } }),
        rift(1, 2, {
          subject: {
            unit: "carrier",
            damaged: true,
            origin: "26",
            cargo: ["infantry", "fighter", "infantry2", "infantry"],
          },
        }),
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
      printed,
    );
    expect(rows).toEqual([
      {
        unit: "cruiser",
        name: "Cruiser II",
        text: "Leaves Gravity Rift · #41 · survives on 4+",
        roll: { face: 5, lost: false },
      },
      {
        unit: "carrier",
        name: "Carrier I (damaged)",
        // Units are counted by the printed unit: an Infantry II is not an Infantry I.
        text: "Leaves Gravity Rift · #41 · from Gravity Rift · #26 · carries 1 Fighter I, 2 Infantry I, 1 Infantry II · survives on 4+",
        roll: { face: 2, lost: true },
      },
    ]);
    expect(riftRows([], label)).toBeNull();
    // Without the catalogue a unit is named by its base unit.
    expect(
      riftRows([rift(0, 5, { subject: { unit: "cruiser2", origin: "41" } })], label)?.[0].name,
    ).toBe("Cruiser");
  });

  it("say what a rift removed: the ship and its cargo", () => {
    expect(
      removedUnits(
        [
          rift(0, 5, { subject: { unit: "cruiser" } }),
          rift(1, 2, { subject: { unit: "carrier", cargo: ["infantry", "infantry2"] } }),
        ],
        printed,
      ),
    ).toEqual(["Carrier I", "Infantry I", "Infantry II"]);
  });

  it("give the space cannon offense a side for each seat that fired and for the ships fired at", () => {
    const ships: ShipsOf = (seat) =>
      seat === "a"
        ? [{ unit: "carrier", name: "Carrier I", count: 1 }]
        : seat === "b"
          ? [{ unit: "destroyer", count: 2 }]
          : [];
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
          subject: { unit: "pds2", target: "a" },
        },
        {
          index: 1,
          roller: "a",
          reason: "space cannon",
          label: "Space cannon",
          faces: [3],
          hits_on: 6,
          hits: 0,
          subject: { unit: "pds", target: "b" },
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
      ships,
      printed,
    );
    expect(table?.score).toBe("1 hit");
    expect(table?.sides.map((side) => [side.seat, side.role, side.isViewer, side.hits])).toEqual([
      ["a", "Attacker", true, 0],
      ["c", "Defender", false, 1],
      ["b", "Defender", false, null],
    ]);
    // The guns of a seat, then the ships of the seat that are fired at.
    expect(table?.sides[0].rows.map((row) => [row.unit, row.label, row.count, row.target])).toEqual(
      [
        ["pds", "PDS I", null, 6],
        ["carrier", "Carrier I", 1, null],
      ],
    );
    expect(table?.sides[1].rows[0]).toMatchObject({
      unit: "pds",
      label: "PDS II",
      count: null,
      target: 6,
      dice: [
        { roll: 8, hit: true },
        { roll: 5, hit: false },
      ],
      hits: "1 hit",
      scored: true,
    });
    expect(table?.sides[2].rows).toMatchObject([{ unit: "destroyer", count: 2, dice: [] }]);
    expect(cannonTable([], "a", "a")).toBeNull();
  });

  it("preview the space cannon offense from the guns of the facts", () => {
    const table = cannonPreview(
      [
        { seat: "c", unit: "pds", dice: 1, hits_on: 6, target: "a" },
        { seat: "c", unit: "pds", dice: 1, hits_on: 6, target: "a" },
        { seat: "d", planet: "lisis", dice: 2, hits_on: 5, target: "a" },
      ],
      "a",
      "a",
      (seat) => (seat === "a" ? [{ unit: "cruiser", count: 2 }] : []),
    );
    expect(table?.score).toBe("4 dice");
    expect(table?.sides.map((side) => [side.seat, side.hits, side.rows.length])).toEqual([
      ["a", null, 1],
      ["c", null, 1],
      ["d", null, 1],
    ]);
    expect(table?.sides[1].rows[0]).toMatchObject({
      unit: "pds",
      count: 2,
      target: 6,
      dice: 2,
      hits: "≈1.0",
    });
    expect(table?.sides[2].rows[0]).toMatchObject({
      unit: null,
      label: "Space cannon · Lisis",
      dice: 2,
    });
    expect(cannonPreview([], "a", "a", () => [])).toBeNull();
  });
});
