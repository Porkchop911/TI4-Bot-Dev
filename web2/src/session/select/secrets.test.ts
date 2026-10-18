import { expect, it } from "vitest";
import menuUpdate from "../../../../fixtures/protocol/update-menu.json";
import type { Catalog, SessionUpdate } from "../wire";
import { selectPlayers } from "./players";
import { selectSecrets } from "./secrets";

// Seat a holds `lsc` and `pe` in this update of the real engine. Nobody has scored a secret. Its
// facts are left out: a test gives its own.
const menu: SessionUpdate = {
  ...(menuUpdate as unknown as SessionUpdate),
  secret_objectives: undefined,
};
const secret = (id: string, name: string, phase: string, group: string) => ({
  id,
  name,
  phase,
  group,
  text: `The text of *${name}*.`,
});
const catalog = {
  components: {
    secret_objectives: [
      secret("btv", "Brave the Void", "Action", "Win a combat"),
      secret("ctr", "Control the Region", "Status", "Ships on the board"),
      secret("dp", "Dictate Policy", "Agenda", "Agenda and table"),
      secret("lsc", "Learn the Secrets of the Cosmos", "Status", "Ships on the board"),
      secret("pe", "Prove Endurance", "Action", "Agenda and table"),
      secret("te", "Threaten Enemies", "Status", "Ships on the board"),
    ],
    secret_objective_groups: [
      "Planets you control",
      "Ships on the board",
      "Win a combat",
      "Agenda and table",
      "Other",
    ],
  },
} as unknown as Catalog;
const withFacts = (scored: Record<string, string[]>, update = menu): SessionUpdate => ({
  ...update,
  secret_objectives: { deck: 25, scored, hand_limit: 3 },
});
/** The update with other secret objectives in the hand of a seat. */
const holding = (seat: string, ids: string[]): SessionUpdate => ({
  ...menu,
  view: {
    ...menu.view,
    players: menu.view.players.map((player) =>
      player.id === seat ? { ...player, held_secret_objectives: ids } : player,
    ),
  },
});

it("shows the hand of the viewer as printed, not scored", () => {
  const view = selectSecrets(withFacts({}), catalog);
  expect(view).toMatchObject({ held: 2, scored: 0, limit: 3, total: 6, unseen: 4, deck: 25 });
  expect(view?.mine).toEqual([
    {
      name: "Learn the Secrets of the Cosmos",
      phase: "Status phase",
      text: "The text of Learn the Secrets of the Cosmos.",
      group: "Ships on the board",
      scored: false,
    },
    {
      name: "Prove Endurance",
      phase: "Action phase",
      text: "The text of Prove Endurance.",
      group: "Agenda and table",
      scored: false,
    },
  ]);
});

it("has a scored objective of the viewer after the hand, and counts it", () => {
  const view = selectSecrets(withFacts({ a: ["btv"] }), catalog);
  expect(view).toMatchObject({ held: 2, scored: 1, unseen: 3 });
  expect(view?.mine.map((card) => [card.name, card.scored])).toEqual([
    ["Learn the Secrets of the Cosmos", false],
    ["Prove Endurance", false],
    ["Brave the Void", true],
  ]);
});

it("lists the deck in the groups of the catalogue, with where each objective is", () => {
  const view = selectSecrets(withFacts({ a: ["btv"], d: ["te"] }), catalog);
  // A group with no objective ("Planets you control", "Other") is left out.
  expect(
    view?.reference.map((group) => [
      group.group,
      group.unseen,
      group.cards.map((card) => [card.name, card.state, card.by, card.mine]),
    ]),
  ).toEqual([
    [
      "Ships on the board",
      1,
      [
        ["Control the Region", "unseen", null, false],
        ["Learn the Secrets of the Cosmos", "held", "a", true],
        ["Threaten Enemies", "scored", "d", false],
      ],
    ],
    ["Win a combat", 0, [["Brave the Void", "scored", "a", true]]],
    [
      "Agenda and table",
      1,
      [
        ["Dictate Policy", "unseen", null, false],
        ["Prove Endurance", "held", "a", true],
      ],
    ],
  ]);
  expect(view?.unseen).toBe(2);
});

it("sees the hand of another seat when the game reveals it", () => {
  const view = selectSecrets(withFacts({}, holding("b", ["ctr"])), catalog);
  const row = view?.reference
    .flatMap((group) => group.cards)
    .find((card) => card.name === "Control the Region");
  expect(row).toMatchObject({ state: "held", by: "b", mine: false });
  expect(view).toMatchObject({ held: 2, unseen: 3 });
});

it("shows an objective that the catalogue does not have by its id", () => {
  const view = selectSecrets(withFacts({ c: ["new_secret"] }), catalog);
  expect(view?.reference.at(-1)).toMatchObject({
    group: "Other",
    cards: [{ name: "New Secret", state: "scored", by: "c" }],
  });
});

it("has the hand without the facts and the catalogue, and nothing for a spectator", () => {
  const view = selectSecrets(menu, null);
  expect(view).toMatchObject({ held: 2, scored: 0, limit: null, deck: null, total: 0 });
  expect(view?.mine.map((card) => card.name)).toEqual(["Lsc", "Pe"]);
  expect(selectSecrets({ ...menu, viewer: { role: "spectator" } }, catalog)).toBeNull();
});

it("counts the scored secrets of a seat in the player table", () => {
  const rows = selectPlayers(withFacts({ d: ["te", "dp"] })).rows;
  expect(rows.find((row) => row.seat === "d")?.secrets).toEqual([2, 2]);
  expect(rows.find((row) => row.seat === "a")?.secrets).toEqual([0, 2]);
});
