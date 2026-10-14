import { describe, expect, it } from "vitest";
import {
  addDestroy,
  addSustain,
  autoFill,
  buildHitRows,
  canAddDestroy,
  canStageHits,
  canSustain,
  casualtyPlan,
  destroyableFromOptions,
  groundHitUnits,
  hitsToCover,
  removeHit,
  spaceHitUnits,
  stagedHits,
  sustainDestroyableFromOptions,
  strandedCargo,
  type HitContext,
} from "./hitAssignment.ts";
import type { PlacedUnitView } from "../protocol/types.ts";

const unit = (
  unit_type: string,
  damaged = false,
  planet?: string,
): PlacedUnitView => ({
  unit_type,
  owner: "p1",
  damaged,
  planet,
});

const fleet = [
  ...Array.from({ length: 10 }, () => unit("fighter")),
  unit("destroyer"),
  unit("destroyer"),
  unit("dreadnought"),
  unit("dreadnought", true),
  unit("carrier"),
  unit("infantry"),
];

const ctx = (extra: Partial<HitContext> = {}): HitContext => ({
  units: spaceHitUnits(fleet, "p1"),
  sustainTypes: new Set(["dreadnought"]),
  ...extra,
});

describe("hit assignment rows", () => {
  it("groups simple ships and lists sustaining and capacity ships one by one", () => {
    const rows = buildHitRows(ctx());
    const fighters = rows.find((r) => r.unitType === "fighter")!;
    expect(fighters).toMatchObject({ individual: false, count: 10 });
    expect(rows.filter((r) => r.unitType === "destroyer")).toHaveLength(1);
    expect(rows.find((r) => r.unitType === "destroyer")!.count).toBe(2);
    const dreadnoughts = rows.filter((r) => r.unitType === "dreadnought");
    expect(dreadnoughts).toHaveLength(2);
    expect(dreadnoughts.every((r) => r.individual)).toBe(true);
    expect(dreadnoughts.find((r) => r.damaged)!.canSustain).toBe(false);
    expect(dreadnoughts.find((r) => !r.damaged)!.canSustain).toBe(true);
    expect(rows.find((r) => r.unitType === "carrier")).toMatchObject({
      individual: true,
      capacity: 4,
    });
    expect(rows.some((r) => r.unitType === "infantry")).toBe(false);
  });

  it("offers only fighters to an anti-fighter barrage", () => {
    const rows = buildHitRows(ctx({ onlyFighters: true }));
    expect(rows.map((r) => r.unitType)).toEqual(["fighter"]);
  });

  it("takes the ground forces on the planet for a ground combat", () => {
    const units = [
      unit("infantry", false, "jord"),
      unit("mech", false, "jord"),
      unit("fighter"),
    ];
    expect(groundHitUnits(units, "p1", "jord").map((u) => u.unit_type)).toEqual(
      ["infantry", "mech"],
    );
  });
});

describe("hit staging", () => {
  it("never stages more hits than owed or ships in a group", () => {
    const c = ctx({ units: [unit("fighter"), unit("fighter")] });
    const [fighters] = buildHitRows(c);
    let staging = addDestroy({}, fighters);
    staging = addDestroy(staging, fighters);
    expect(canAddDestroy(fighters, staging, 5, c)).toBe(false);
    expect(hitsToCover(5, buildHitRows(c), c)).toBe(2);
    expect(canAddDestroy(fighters, addDestroy({}, fighters), 1, c)).toBe(false);
  });

  it("moves a hit from one ship to another", () => {
    const c = ctx();
    const rows = buildHitRows(c);
    const fighters = rows.find((r) => r.unitType === "fighter")!;
    const destroyers = rows.find((r) => r.unitType === "destroyer")!;
    let staging = addDestroy({}, fighters);
    staging = removeHit(staging, fighters);
    staging = addDestroy(staging, destroyers);
    expect(casualtyPlan(rows, staging)).toEqual([
      { kind: "destroy", unit: "destroyer", damaged: false },
    ]);
  });

  it("only destroys ships the engine offers", () => {
    const c = ctx({ destroyable: new Set(["fighter|intact"]) });
    const rows = buildHitRows(c);
    const dread = rows.find((r) => r.unitType === "dreadnought" && !r.damaged)!;
    expect(canAddDestroy(dread, addSustain({}, dread), 3, c)).toBe(false);
    const destroyer = rows.find((r) => r.unitType === "destroyer")!;
    expect(canAddDestroy(destroyer, {}, 3, c)).toBe(false);
  });

  it("costs two hits to destroy an undamaged sustaining ship: sustain, then destroy", () => {
    const c = ctx({ units: [unit("dreadnought")] });
    const rows = buildHitRows(c);
    const [dread] = rows;
    expect(canSustain(dread, {}, 1)).toBe(true);
    expect(canAddDestroy(dread, {}, 1, c)).toBe(false);
    expect(canAddDestroy(dread, {}, 2, c)).toBe(true);
    const staging = addDestroy({}, dread);
    expect(staging[dread.key]).toEqual({ destroy: 1, sustain: true });
    expect(stagedHits(staging)).toBe(2);
    expect(casualtyPlan(rows, staging)).toEqual([
      { kind: "sustain", unit: "dreadnought" },
      { kind: "destroy", unit: "dreadnought", damaged: true },
    ]);
    expect(canAddDestroy(dread, staging, 5, c)).toBe(false);
  });

  it("lets a sustained ship take a second hit that destroys it, while a hit remains", () => {
    const c = ctx({ units: [unit("dreadnought")] });
    const [dread] = buildHitRows(c);
    const sustained = addSustain({}, dread);
    expect(canSustain(dread, sustained, 2)).toBe(false);
    expect(canAddDestroy(dread, sustained, 2, c)).toBe(true);
    expect(canAddDestroy(dread, sustained, 1, c)).toBe(false);
  });

  it("destroys a damaged sustaining ship with one hit", () => {
    const c = ctx({ units: [unit("dreadnought", true)] });
    const rows = buildHitRows(c);
    const [dread] = rows;
    expect(dread.canSustain).toBe(false);
    expect(canAddDestroy(dread, {}, 1, c)).toBe(true);
    expect(casualtyPlan(rows, addDestroy({}, dread))).toEqual([
      { kind: "destroy", unit: "dreadnought", damaged: true },
    ]);
  });

  it("undoes the destroy before the sustain", () => {
    const c = ctx({ units: [unit("dreadnought")] });
    const [dread] = buildHitRows(c);
    let staging = addDestroy({}, dread);
    staging = removeHit(staging, dread);
    expect(staging[dread.key]).toEqual({ destroy: 0, sustain: true });
    staging = removeHit(staging, dread);
    expect(staging[dread.key]).toEqual({ destroy: 0, sustain: false });
  });

  it("counts an undamaged sustaining ship as two assignable hits", () => {
    const one = ctx({ units: [unit("dreadnought")] });
    expect(hitsToCover(9, buildHitRows(one), one)).toBe(2);
    const damaged = ctx({ units: [unit("dreadnought", true)] });
    expect(hitsToCover(9, buildHitRows(damaged), damaged)).toBe(1);
    const mixed = ctx({ units: [unit("dreadnought"), unit("fighter")] });
    expect(hitsToCover(9, buildHitRows(mixed), mixed)).toBe(3);
  });

  it("puts every sustain before the losses in the plan", () => {
    const c = ctx();
    const rows = buildHitRows(c);
    const fighters = rows.find((r) => r.unitType === "fighter")!;
    const dread = rows.find((r) => r.unitType === "dreadnought" && !r.damaged)!;
    const damaged = rows.find(
      (r) => r.unitType === "dreadnought" && r.damaged,
    )!;
    let staging = addDestroy({}, fighters);
    staging = addDestroy(staging, damaged);
    staging = addSustain(staging, dread);
    expect(casualtyPlan(rows, staging)).toEqual([
      { kind: "sustain", unit: "dreadnought" },
      { kind: "destroy", unit: "fighter", damaged: false },
      { kind: "destroy", unit: "dreadnought", damaged: true },
    ]);
  });

  it("auto-assigns sustains first, then the cheapest ships", () => {
    const c = ctx();
    const rows = buildHitRows(c);
    const plan = casualtyPlan(rows, autoFill(rows, {}, 3, c));
    expect(plan).toEqual([
      { kind: "sustain", unit: "dreadnought" },
      { kind: "destroy", unit: "fighter", damaged: false },
      { kind: "destroy", unit: "fighter", damaged: false },
    ]);
  });

  it("auto-assign spends every hit it legally can, destroying a sustained ship last", () => {
    const lone = ctx({ units: [unit("dreadnought")] });
    const loneRows = buildHitRows(lone);
    expect(casualtyPlan(loneRows, autoFill(loneRows, {}, 1, lone))).toEqual([
      { kind: "sustain", unit: "dreadnought" },
    ]);
    expect(casualtyPlan(loneRows, autoFill(loneRows, {}, 2, lone))).toEqual([
      { kind: "sustain", unit: "dreadnought" },
      { kind: "destroy", unit: "dreadnought", damaged: true },
    ]);
    const c = ctx();
    const rows = buildHitRows(c);
    const filled = autoFill(rows, {}, 5, c);
    expect(stagedHits(filled)).toBe(5);
  });

  it("warns when lost capacity strands cargo", () => {
    const c = ctx({ units: [unit("carrier"), unit("fighter")] });
    const rows = buildHitRows(c);
    const carrier = rows.find((r) => r.unitType === "carrier")!;
    // 5 cargo against 4 capacity is already one over; losing the carrier strands 4 more.
    expect(strandedCargo(rows, addDestroy({}, carrier), 5, 4)).toBe(4);
    expect(strandedCargo(rows, {}, 3, 4)).toBe(0);
  });

  it("reads what a casualty decision lets the seat destroy", () => {
    expect(
      destroyableFromOptions([
        {
          id: "destroy|0",
          kind: "casualty",
          label: "",
          payload: { unit: "carrier", damaged: false },
        },
        {
          id: "destroy|1",
          kind: "ground_casualty",
          label: "",
          payload: { unit: "mech", damaged: true },
        },
        { id: "decline", kind: "decline", label: "" },
      ]),
    ).toEqual(new Set(["carrier|intact", "mech|damaged"]));
  });
});

describe("hits bound to non-fighter ships (Graviton Laser System)", () => {
  // Nightly runs 07-2221 and 37-0225: on the sustain question the panel let the seat lose a
  // fighter, and the engine's next question (non-fighters only) rejected the plan.
  const sustainOptions = (bound: boolean) => [
    {
      id: "sustain|5",
      kind: "sustain",
      label: "",
      payload: { unit: "dreadnought" },
    },
    {
      id: "decline",
      kind: "decline",
      label: "",
      payload: bound ? { non_fighters_first: true } : {},
    },
  ];
  const units = [unit("destroyer"), unit("dreadnought"), unit("fighter")];

  it("does not let the seat lose a fighter while a non-fighter ship can take the hit", () => {
    const c = ctx({
      units,
      destroyable: sustainDestroyableFromOptions(sustainOptions(true), units),
    });
    const rows = buildHitRows(c);
    const fighter = rows.find((r) => r.unitType === "fighter")!;
    expect(canAddDestroy(fighter, {}, 1, c)).toBe(false);
    const destroyer = rows.find((r) => r.unitType === "destroyer")!;
    expect(canAddDestroy(destroyer, {}, 1, c)).toBe(true);
    const dread = rows.find((r) => r.unitType === "dreadnought")!;
    expect(canAddDestroy(dread, {}, 2, c)).toBe(true);
    expect(casualtyPlan(rows, autoFill(rows, {}, 2, c))).toEqual([
      { kind: "sustain", unit: "dreadnought" },
      { kind: "destroy", unit: "destroyer", damaged: false },
    ]);
  });

  it("leaves every ship destroyable when the hits are not bound", () => {
    expect(
      sustainDestroyableFromOptions(sustainOptions(false), units),
    ).toBeNull();
    const fightersOnly = [unit("fighter")];
    expect(
      sustainDestroyableFromOptions(sustainOptions(true), fightersOnly),
    ).toBeNull();
  });
});

describe("canStageHits", () => {
  it("is false when the seat has no unit that can take a hit, so the old controls stay", () => {
    expect(canStageHits({ units: [], sustainTypes: new Set() })).toBe(false);
    expect(
      canStageHits({
        units: spaceHitUnits([unit("infantry", false, "p")], "p1"),
        sustainTypes: new Set(),
      }),
    ).toBe(false);
  });

  it("is true for a seat with a sustaining ship on an unstated-amount sustain decision", () => {
    expect(
      canStageHits({
        units: [unit("dreadnought")],
        sustainTypes: new Set(["dreadnought"]),
      }),
    ).toBe(true);
  });
});
