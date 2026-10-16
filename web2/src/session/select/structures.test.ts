import { describe, expect, it } from "vitest";
import type { Choice, SessionUpdate, StructureHints, StructureSpot } from "../wire";
import {
  chooseStructure,
  chooseStructurePlanet,
  chooseStructureSystem,
  clearLastStructure,
  clearStructurePart,
  selectStructures,
} from "./structures";

// The questions of Construction as the engine asks them (the scenarios "strategy_construction"
// and "strategy_construction_te"): Qucen'n has a PDS, so a first PDS there fills it.
const spot = (unit: string, planet: string, system: string): StructureSpot => ({
  id: `${unit}|${system}|${planet}`,
  unit,
  planet,
  system,
});
const FIRST = [
  spot("pds", "jord", "01"),
  spot("pds", "qucenn", "29"),
  spot("spacedock", "qucenn", "29"),
  spot("pds", "rarron", "29"),
  spot("spacedock", "rarron", "29"),
];
const PDS = FIRST.filter((item) => item.unit === "pds");
const PLANETS: StructureHints["planets"] = {
  jord: { pds: 0, spacedock: 1 },
  qucenn: { pds: 1, spacedock: 0 },
  rarron: { pds: 0, spacedock: 0 },
};
const SUPPLY: StructureHints["supply"] = {
  pds: { placed: 1, limit: 6 },
  spacedock: { placed: 1, limit: 3 },
};
const hints = (
  second: { only_pds: boolean; options: StructureSpot[] } | null,
  step = true,
): StructureHints => ({
  placements: [
    {
      step: step ? 1 : null,
      of: step ? 2 : null,
      only_pds: false,
      options: FIRST,
      without: {},
    },
    ...(second
      ? [
          {
            step: 2,
            of: 2,
            only_pds: second.only_pds,
            options: second.options,
            without: { "pds|29|qucenn": ["pds|29|qucenn"] },
          },
        ]
      : []),
  ],
  planets: PLANETS,
  supply: SUPPLY,
});
const placing = (details: Record<string, unknown>): Choice => ({
  player: "a",
  prompt: "place a structure",
  options: [
    ...FIRST.map((item) => ({
      id: item.id,
      kind: "build",
      label: `place ${item.unit} on ${item.planet}`,
      payload: { planet: item.planet, system: item.system, unit: item.unit },
    })),
    { id: "decline", kind: "decline", label: "Decline", payload: {} },
  ],
  details,
});
const primary = placing({ step: 1, of: 2, structures: hints({ only_pds: true, options: PDS }) });
const ability: Choice = {
  player: "a",
  prompt: "Construction: a structure or a production",
  options: [
    { id: "structure", kind: "build", label: "place a structure", payload: {} },
    { id: "produce|01", kind: "build", label: "use PRODUCTION in 01", payload: {} },
  ],
  details: { structures: hints({ only_pds: false, options: FIRST }) },
};
const window: Choice = {
  player: "b",
  prompt: "spend a strategy token to build a structure",
  options: [
    { id: "no", kind: "strategy", label: "decline", payload: {} },
    { id: "yes", kind: "strategy", label: "build", payload: {} },
  ],
  details: {
    kind: "strategy_secondary",
    structures: hints(null, false),
    tokens_left: 2,
    costs_token: true,
  },
};
const update = {
  viewer: { role: "player", seat: "a" },
  view: {
    players: [{ id: "a" }, { id: "b" }],
    board: {
      map_tiles: [
        { system_id: "01", label: "Jord", planets: [{ id: "jord", label: "Jord" }] },
        {
          system_id: "29",
          label: "Qucen'n",
          planets: [
            { id: "qucenn", label: "Qucen'n" },
            { id: "rarron", label: "Rarron" },
          ],
        },
      ],
    },
  },
} as unknown as SessionUpdate;

describe("selectStructures", () => {
  it("is null for a question with no structures", () => {
    expect(selectStructures(update, { ...primary, details: { step: 1, of: 2 } }, null)).toBeNull();
    expect(
      selectStructures(update, { ...window, details: { kind: "strategy_secondary" } }, null),
    ).toBeNull();
  });

  it("chooses the system on the map, then a planet and a structure in the panel", () => {
    const empty = selectStructures(update, primary, null)!;
    expect(empty.blocks[0]).toMatchObject({
      kind: "gauges",
      gauges: [
        { label: "Space docks on the board", used: 1, total: 3 },
        { label: "PDS on the board", used: 1, total: 6 },
      ],
    });
    expect(empty.blocks[1]).toMatchObject({
      kind: "card",
      title: "Structures",
      rows: [
        { title: ["1 of 2 · space dock or PDS"], subtitle: "No planet chosen" },
        { title: ["2 of 2 · PDS only"], subtitle: "No planet chosen" },
      ],
    });
    expect(empty.blocks[2]).toMatchObject({
      kind: "card",
      title: "System",
      aside: "Choose on the map",
      rows: [{ title: ["No system chosen"] }],
    });
    // The systems where the first placement has an option, with the count of their planets.
    expect(empty.task).toMatchObject({
      target: "system",
      interactive: true,
      values: { "01": 1, "29": 2 },
      chosen: {},
    });
    expect(empty.note).toBe("Choose a system on the map.");
    expect(empty.label).toBe("Resolve primary · no structure");
    expect(empty.answers).toEqual(["decline", "decline"]);
    expect(chooseStructureSystem(primary, null, "03")).toBeNull();
    // A planet of the map chooses its system.
    expect(chooseStructurePlanet(primary, null, "rarron")?.system).toBe("29");

    let draft = chooseStructureSystem(primary, null, "29")!;
    const system = selectStructures(update, primary, draft)!;
    // The system is chosen: the choice is in the panel, the map only shows it.
    expect(system.task).toMatchObject({ interactive: false, context: ["29"] });
    expect(system.note).toBe("Choose a planet and a structure.");
    expect(system.blocks[2]).toMatchObject({
      rows: [{ title: ["Qucen'n · #29"], control: { kind: "button" } }],
    });
    expect(system.blocks[3]).toMatchObject({
      kind: "menu",
      title: "Structure 1 of 2 · space dock or PDS",
      rows: [
        { group: "Qucen'n", key: "1", title: "Space dock", state: "0 → 1", disabled: false },
        { key: "2", title: "PDS", state: "1 → 2", disabled: false },
        { group: "Rarron", key: "3", title: "Space dock", state: "0 → 1", disabled: false },
        { key: "4", title: "PDS", state: "0 → 1", disabled: false },
      ],
    });

    draft = chooseStructure(primary, draft, "place", "pds|29|qucenn")!;
    // Rarron takes the second PDS: the system stays in the panel.
    expect(draft.system).toBe("29");
    const first = selectStructures(update, primary, draft)!;
    expect(first.blocks[1]).toMatchObject({
      rows: [
        { title: ["Qucen'n · #29"], subtitle: "PDS 1 → 2" },
        { title: ["2 of 2 · PDS only"], subtitle: "No planet chosen" },
      ],
    });
    expect(first.blocks[3]).toMatchObject({
      title: "Structure 2 of 2 · PDS only",
      rows: [
        { title: "Space dock", state: "PDS only", disabled: true },
        // The staged PDS: the second one is not offered where the first fills the planet.
        { title: "PDS", state: "1 → 2", selected: true, disabled: false },
        { title: "Space dock", state: "PDS only", disabled: true },
        { title: "PDS", state: "0 → 1", selected: false, disabled: false },
      ],
    });
    expect(first.task?.chosen).toEqual({ "29": true });
    // The staged row takes it back.
    const taken = chooseStructure(primary, draft, "place", "pds|29|qucenn")!;
    expect(selectStructures(update, primary, taken)!.answers).toEqual(["decline", "decline"]);

    draft = chooseStructure(primary, draft, "place", "pds|29|rarron")!;
    const both = selectStructures(update, primary, draft)!;
    expect(both.label).toBe("Resolve primary");
    expect(both.note).toBe("PDS on Qucen'n · PDS on Rarron");
    expect(both.answers).toEqual(["pds|29|qucenn", "pds|29|rarron"]);
    expect(both.blocks[0]).toMatchObject({ gauges: [{ used: 1 }, { used: 3 }] });
    expect(both.blocks[3]).toMatchObject({
      title: "Planets",
      rows: [
        { state: "Every structure is placed", disabled: true },
        { selected: true },
        { state: "Every structure is placed", disabled: true },
        { selected: true },
      ],
    });

    // "Change" of a structure opens it again in its system; "Change" of the system is the map.
    const changed = clearStructurePart(primary, draft, "structure:0")!;
    expect(changed.system).toBe("29");
    expect(selectStructures(update, primary, changed)!.answers).toEqual([
      "decline",
      "pds|29|rarron",
    ]);
    const map = clearStructurePart(primary, draft, "system")!;
    expect(map.system).toBeNull();
    // Esc clears the last structure, then the system.
    const back = clearLastStructure(primary, draft)!;
    expect(selectStructures(update, primary, back)!.answers).toEqual(["pds|29|qucenn", "decline"]);
    const none = clearLastStructure(primary, back)!;
    expect(none.system).toBe("29");
    const cleared = clearLastStructure(primary, none)!;
    expect(cleared.system).toBeNull();
    expect(clearLastStructure(primary, cleared)).toBeNull();
  });

  it("says why a structure is not offered, and the map chooses the next system", () => {
    const narrow = placing({
      step: 1,
      of: 2,
      structures: {
        ...hints(null),
        placements: [
          { step: 1, of: 2, only_pds: false, options: FIRST, without: {} },
          {
            step: 2,
            of: 2,
            only_pds: true,
            options: [spot("pds", "jord", "01"), spot("pds", "qucenn", "29")],
            without: { "spacedock|29|qucenn": ["pds|29|qucenn"] },
          },
        ],
      },
    });
    let draft = chooseStructureSystem(narrow, null, "29")!;
    draft = chooseStructure(narrow, draft, "place", "spacedock|29|qucenn")!;
    const view = selectStructures(update, narrow, { ...draft, system: "29" })!;
    expect(view.blocks[3]).toMatchObject({
      rows: [
        { title: "Space dock", selected: true },
        { title: "PDS", state: "Not after the space dock on Qucen'n", disabled: true },
        { title: "Space dock", state: "PDS only", disabled: true },
        { title: "PDS", state: "Not offered", disabled: true },
      ],
    });
    // Nothing of the second placement is left in #29: its system is chosen on the map.
    expect(draft.system).toBeNull();
    const next = selectStructures(update, narrow, draft)!;
    expect(next.task).toMatchObject({
      interactive: true,
      values: { "01": 1 },
      chosen: { "29": true },
    });
  });

  it("chooses the ability first: a structure places two, a production is sent alone", () => {
    const empty = selectStructures(update, ability, null)!;
    expect(empty.problem).toBe("Choose an ability: a structure or a PRODUCTION.");
    expect(empty.task).toBeNull();
    expect(empty.blocks[0]).toMatchObject({
      kind: "menu",
      title: "Ability",
      rows: [{ title: "Place a structure" }, { title: "Use PRODUCTION · Jord · #01" }],
    });
    const producing = chooseStructure(ability, null, "ability", "produce|01")!;
    const production = selectStructures(update, ability, producing)!;
    expect(production.label).toBe("Resolve primary · use PRODUCTION");
    expect(production.answers).toEqual(["produce|01"]);
    expect(production.task).toBeNull();

    let draft = chooseStructure(ability, null, "ability", "structure")!;
    expect(selectStructures(update, ability, draft)!.task).toMatchObject({ interactive: true });
    draft = chooseStructureSystem(ability, draft, "29")!;
    draft = chooseStructure(ability, draft, "place", "spacedock|29|rarron")!;
    // The second structure can be either kind; the keys go on after the ability.
    expect(selectStructures(update, ability, draft)!.blocks).toContainEqual(
      expect.objectContaining({
        kind: "menu",
        title: "Structure 2 of 2 · space dock or PDS",
        rows: expect.arrayContaining([expect.objectContaining({ key: "3", group: "Qucen'n" })]),
      }),
    );
    draft = clearStructurePart(ability, draft, "system")!;
    draft = chooseStructureSystem(ability, draft, "01")!;
    draft = chooseStructure(ability, draft, "place", "pds|01|jord")!;
    expect(selectStructures(update, ability, draft)!.answers).toEqual([
      "structure",
      "spacedock|29|rarron",
      "pds|01|jord",
    ]);
  });

  it("the window of the secondary says what it costs; nothing chosen is a pass", () => {
    const empty = selectStructures(update, window, null)!;
    expect(empty.label).toBe("Pass");
    expect(empty.answers).toEqual(["no"]);
    expect(empty.note).toBe("Nothing chosen: you pass.");
    expect(empty.blocks.find((block) => block.kind === "card")).toMatchObject({
      title: "Structure",
      aside: "Strategy pool 2 → 1",
      rows: [{ title: ["Space dock or PDS"] }],
    });
    let draft = chooseStructureSystem(window, null, "29")!;
    expect(selectStructures(update, window, draft)!.blocks).toContainEqual(
      expect.objectContaining({ kind: "menu", title: "Structure" }),
    );
    draft = chooseStructure(window, draft, "place", "spacedock|29|qucenn")!;
    const followed = selectStructures(update, window, draft)!;
    expect(followed.label).toBe("Follow · place space dock");
    expect(followed.answers).toEqual(["yes", "spacedock|29|qucenn"]);
  });
});
