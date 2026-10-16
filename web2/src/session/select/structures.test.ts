import { describe, expect, it } from "vitest";
import type { Choice, SessionUpdate } from "../wire";
import {
  chooseStructure,
  clearLastStructure,
  clearStructurePart,
  placeStructure,
  selectStructures,
} from "./structures";

// The questions of Construction as the engine asks them (the scenarios "strategy_construction"
// and "strategy_construction_te"): Qucen'n has a PDS, so a first PDS there fills it.
const spot = (unit: string, planet: string, system: string) => ({
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
const PLANETS = {
  jord: { pds: 0, spacedock: 1 },
  qucenn: { pds: 1, spacedock: 0 },
  rarron: { pds: 0, spacedock: 0 },
};
const SUPPLY = { pds: { placed: 1, limit: 6 }, spacedock: { placed: 1, limit: 3 } };
const hints = (second: { only_pds: boolean; options: typeof FIRST } | null, step = true) => ({
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

  it("chooses the kind, then the planet on the map, and answers both placements at once", () => {
    const empty = selectStructures(update, primary, null)!;
    expect(empty.blocks[0]).toMatchObject({
      kind: "gauges",
      gauges: [
        { label: "Space docks on the board", used: 1, total: 3 },
        { label: "PDS on the board", used: 1, total: 6 },
      ],
    });
    expect(empty.blocks[1]).toMatchObject({ kind: "menu", title: "Structure 1 of 2" });
    expect(empty.task).toBeNull();
    expect(empty.label).toBe("Resolve primary · no structure");
    expect(empty.answers).toEqual(["decline", "decline"]);
    expect(placeStructure(primary, null, "rarron")).toBeNull();

    let draft = chooseStructure(primary, null, "structure:0", "pds")!;
    const kind = selectStructures(update, primary, draft)!;
    expect(kind.task).toMatchObject({
      target: "planet",
      values: { jord: 0, qucenn: 1, rarron: 0 },
      verb: "Place PDS on",
    });
    draft = placeStructure(primary, draft, "qucenn")!;
    const first = selectStructures(update, primary, draft)!;
    const rows = first.blocks.find((block) => block.kind === "card");
    expect(rows).toMatchObject({
      title: "Structures",
      rows: [
        { title: ["Qucen'n · #29"], subtitle: "PDS 1 → 2" },
        { title: ["2 of 2 · PDS only"], subtitle: "No planet chosen" },
      ],
    });
    // The second PDS of the card is not offered where the first one fills the planet.
    expect(first.task?.values).toEqual({ qucenn: 0, jord: 0, rarron: 0 });
    expect(first.task?.chosen).toEqual({ qucenn: true });
    expect(placeStructure(primary, draft, "qucenn")).toBeNull();
    draft = placeStructure(primary, draft, "rarron")!;
    const both = selectStructures(update, primary, draft)!;
    expect(both.label).toBe("Resolve primary");
    expect(both.note).toBe("PDS on Qucen'n · PDS on Rarron");
    expect(both.answers).toEqual(["pds|29|qucenn", "pds|29|rarron"]);
    expect(both.blocks[0]).toMatchObject({ gauges: [{ used: 1 }, { used: 3 }] });

    // "Change" gives the planet back; Esc clears the last planet, then the kind.
    const changed = clearStructurePart(primary, draft, "structure:0")!;
    expect(selectStructures(update, primary, changed)!.answers).toEqual([
      "decline",
      "pds|29|rarron",
    ]);
    const back = clearLastStructure(primary, draft)!;
    expect(selectStructures(update, primary, back)!.answers).toEqual(["pds|29|qucenn", "decline"]);
    const none = clearLastStructure(primary, clearLastStructure(primary, back)!)!;
    expect(none.units.every((unit) => !unit)).toBe(true);
    expect(clearLastStructure(primary, none)).toBeNull();
  });

  it("chooses the ability first: a structure places two, a production is sent alone", () => {
    const empty = selectStructures(update, ability, null)!;
    expect(empty.problem).toBe("Choose an ability: a structure or a PRODUCTION.");
    expect(empty.blocks[0]).toMatchObject({
      kind: "menu",
      title: "Ability",
      rows: [{ title: "Place a structure" }, { title: "Use PRODUCTION · Jord · #01" }],
    });
    const producing = chooseStructure(ability, null, "ability", "produce|01")!;
    const production = selectStructures(update, ability, producing)!;
    expect(production.label).toBe("Resolve primary · use PRODUCTION");
    expect(production.answers).toEqual(["produce|01"]);

    let draft = chooseStructure(ability, null, "ability", "structure")!;
    draft = chooseStructure(ability, draft, "structure:0", "spacedock")!;
    draft = placeStructure(ability, draft, "rarron")!;
    // The second structure can be either kind: its kind is chosen too.
    expect(selectStructures(update, ability, draft)!.blocks).toContainEqual(
      expect.objectContaining({ kind: "menu", title: "Structure 2 of 2" }),
    );
    draft = chooseStructure(ability, draft, "structure:1", "pds")!;
    draft = placeStructure(ability, draft, "jord")!;
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
    expect(empty.blocks.find((block) => block.kind === "card")).toMatchObject({
      title: "Planet",
      aside: "Choose on the map · Strategy pool 2 → 1",
      rows: [{ title: ["Space dock or PDS"] }],
    });
    let draft = chooseStructure(window, null, "structure:0", "spacedock")!;
    draft = placeStructure(window, draft, "qucenn")!;
    const followed = selectStructures(update, window, draft)!;
    expect(followed.label).toBe("Follow · place space dock");
    expect(followed.answers).toEqual(["yes", "spacedock|29|qucenn"]);
  });
});
