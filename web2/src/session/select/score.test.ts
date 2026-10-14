import { expect, it } from "vitest";
import menuUpdate from "../fixtures/update-menu.json";
import type { Catalog, ScoreFacts, SessionUpdate } from "../wire";
import { selectScore } from "./score";

// An update of the real engine, viewed by seat a, with the points written by hand.
const menu = menuUpdate as unknown as SessionUpdate;
const seats = menu.view.players.map((player) => player.id);
const facts: ScoreFacts = {
  target: 10,
  public: [
    {
      id: "expand_borders",
      name: "Expand Borders",
      points: 1,
      text: "Control *6* planets.",
      scored: ["a", "c"],
    },
    {
      id: "master_science",
      name: "Master the Sciences",
      points: 2,
      text: "Own 2 technologies.",
      scored: ["b"],
    },
  ],
  sources: [
    { kind: "secrets", name: "", points: { a: 2, d: 1 } },
    { kind: "custodians", name: "", points: { b: 1 } },
    { kind: "imperial", name: "", points: {} },
    { kind: "support", name: "", points: { a: 1 }, from: { a: ["c"] } },
    { kind: "agenda", name: "Mutiny", points: { d: -1 } },
    { kind: "relic", name: "Shard of the Throne", points: { c: 1 } },
    { kind: "other", name: "a reason of tomorrow", points: { c: 1 } },
  ],
};
const score = (score: ScoreFacts = facts) => selectScore({ ...menu, score }, null);
const cells = (view: ReturnType<typeof score>, name: string) => {
  const row = view?.groups.flatMap((group) => group.rows).find((item) => item.name === name);
  return Object.fromEntries(
    (view?.seats ?? []).map((seat, index) => [seat.seat, row?.cells[index]]),
  );
};

it("has no sheet when the game does not say where the points come from", () => {
  expect(selectScore(menu, null)).toBeNull();
});

it("has the viewer first, then the other seats by their points", () => {
  const players = menu.view.players.map((player, index) => ({
    ...player,
    victory_points: index % 4,
  }));
  const view = selectScore({ ...menu, view: { ...menu.view, players }, score: facts }, null);
  expect(view?.target).toBe(10);
  expect(view?.seats.map((seat) => seat.seat).sort()).toEqual([...seats].sort());
  expect(view?.seats.map((seat) => seat.isMe)).toEqual([true, ...seats.slice(1).map(() => false)]);
  expect(view?.seats[0].seat).toBe("a");
  const points = view?.seats.slice(1).map((seat) => seat.points) ?? [];
  expect(points).toEqual([...points].sort((a, b) => b - a));
  for (const seat of view?.seats ?? []) {
    expect(seat.points).toBe(players.find((player) => player.id === seat.seat)?.victory_points);
  }
});

it("puts the public objectives in their stage, with the seats that scored", () => {
  const view = score();
  expect(view?.groups.map((group) => [group.title, group.rows.map((row) => row.name)])).toEqual([
    ["Public objectives · stage I", ["Expand Borders"]],
    ["Public objectives · stage II", ["Master the Sciences"]],
    [
      "Other points",
      [
        "Secret objectives",
        "Custodians of Mecatol Rex",
        "Imperial · Mecatol Rex",
        "Support for the Throne",
        "Agenda · Mutiny",
        "Relic · Shard of the Throne",
        "a reason of tomorrow",
      ],
    ],
  ]);
  const borders = view?.groups[0].rows[0];
  expect(borders).toMatchObject({ worth: 1, text: "Control 6 planets." });
  expect(cells(view, "Expand Borders")).toMatchObject({
    a: { kind: "scored" },
    b: { kind: "none" },
    c: { kind: "scored" },
  });
  expect(view?.groups[1].rows[0].worth).toBe(2);
});

it("has one cell for each seat in every row", () => {
  for (const row of score()?.groups.flatMap((group) => group.rows) ?? []) {
    expect(row.cells).toHaveLength(seats.length);
  }
});

it("says the points of another source, a loss, and whose Support a seat has", () => {
  const view = score();
  expect(cells(view, "Secret objectives")).toMatchObject({
    a: { kind: "points", value: 2, from: [], detail: [] },
    b: { kind: "none" },
    d: { kind: "points", value: 1 },
  });
  expect(cells(view, "Imperial · Mecatol Rex").a).toEqual({ kind: "none" });
  expect(cells(view, "Support for the Throne").a).toEqual({
    kind: "points",
    value: 1,
    from: ["c"],
    detail: [],
  });
  expect(cells(view, "Agenda · Mutiny").d).toEqual({
    kind: "points",
    value: -1,
    from: [],
    detail: [],
  });
});

it("has a row for the points that no source explains", () => {
  const view = score({ ...facts, unexplained: { b: 2 } });
  expect(view?.groups[2].rows.at(-1)?.name).toBe("Other");
  expect(cells(view, "Other").b).toEqual({ kind: "points", value: 2, from: [], detail: [] });
});

it("says which rows still have a point to get or to lose", () => {
  const open = (view: ReturnType<typeof score>) =>
    view?.groups.flatMap((group) => group.rows.filter((row) => row.open).map((row) => row.name));
  // The custodians are taken and Imperial is done. The objectives and the notes stay, and the
  // agenda: a law can be repealed.
  expect(open(score())).toEqual([
    "Expand Borders",
    "Master the Sciences",
    "Support for the Throne",
    "Agenda · Mutiny",
  ]);
  // The custodians that no seat has are still to get, and an objective that every seat scored is not.
  const sources = facts.sources.map((source) =>
    source.kind === "custodians" ? { ...source, points: {} } : source,
  );
  const all = [{ ...facts.public[0], scored: seats }, facts.public[1]];
  expect(open(score({ ...facts, sources, public: all }))).toEqual([
    "Master the Sciences",
    "Custodians of Mecatol Rex",
    "Support for the Throne",
    "Agenda · Mutiny",
  ]);
});

it("names the scored secret objectives of a seat", () => {
  const view = selectScore(
    {
      ...menu,
      score: facts,
      secret_objectives: { deck: 20, scored: { a: ["cut_supply_lines"] }, hand_limit: 3 },
    },
    null,
  );
  expect(cells(view, "Secret objectives").a).toMatchObject({
    detail: [{ name: "Cut Supply Lines", text: "" }],
  });
});

it("has the printed text of a source that has one", () => {
  const sources = facts.sources.map((source) =>
    source.kind === "agenda" ? { ...source, text: "For: *gain* 1.\nAgainst: lose 1." } : source,
  );
  const rows = score({ ...facts, sources })?.groups[2].rows ?? [];
  expect(rows.find((row) => row.name === "Agenda · Mutiny")?.text).toBe(
    "For: gain 1.\nAgainst: lose 1.",
  );
  expect(rows.find((row) => row.name === "Support for the Throne")?.text).toBe("");
});

it("lists the public objectives that can still be revealed, in the groups of the catalogue", () => {
  const card = (id: string, name: string, points: number, group: string) => ({
    id,
    name,
    points,
    group,
    text: `*${name}*.`,
  });
  const catalog = {
    components: {
      secret_objectives: [],
      public_objectives: [
        card("expand_borders", "Expand Borders", 1, "Planets you control"),
        card("sway_council", "Sway the Council", 1, "Spend"),
        card("master_science", "Master the Sciences", 2, "Technology"),
        card("golden_age", "Found a Golden Age", 2, "Spend"),
        card("subdue", "Subdue the Galaxy", 2, "Planets you control"),
      ],
      public_objective_groups: ["Spend", "Planets you control", "Technology", "Other"],
    },
  } as unknown as Catalog;
  const deck = (score: ScoreFacts) => selectScore({ ...menu, score }, catalog)?.deck;
  // Stage I only is revealed: what is on the table is not listed again, and both stages can come.
  expect(deck({ ...facts, public: [facts.public[0]] })).toEqual([
    {
      group: "Spend",
      cards: [
        { name: "Sway the Council", text: "Sway the Council.", worth: 1 },
        { name: "Found a Golden Age", text: "Found a Golden Age.", worth: 2 },
      ],
    },
    {
      group: "Planets you control",
      cards: [{ name: "Subdue the Galaxy", text: "Subdue the Galaxy.", worth: 2 }],
    },
    {
      group: "Technology",
      cards: [{ name: "Master the Sciences", text: "Master the Sciences.", worth: 2 }],
    },
  ]);
  // With an objective of stage II on the table, none of stage I comes any more.
  expect(deck(facts)?.map((group) => [group.group, group.cards.map((item) => item.name)])).toEqual([
    ["Spend", ["Found a Golden Age"]],
    ["Planets you control", ["Subdue the Galaxy"]],
  ]);
  // Without the catalogue the sheet has no deck.
  expect(score()?.deck).toEqual([]);
});
