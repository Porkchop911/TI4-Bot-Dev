import { expect, it } from "vitest";
import menuUpdate from "../../../../fixtures/protocol/update-menu.json";
import type { Catalog, SessionUpdate } from "../wire";
import { selectActionCards } from "./cards";

// An update of the real engine, with seat a holding `scuttle` and `rally`. Its facts are left out:
// a test gives its own.
const update = menuUpdate as unknown as SessionUpdate;
const menu: SessionUpdate = {
  ...update,
  view: {
    ...update.view,
    players: update.view.players.map((player) =>
      player.id === "a"
        ? { ...player, held_action_cards: ["scuttle", "rally"], action_cards_count: 2 }
        : player,
    ),
  },
  action_cards: undefined,
};
const card = (id: string, name: string, window: string, group: string) => ({
  id,
  name,
  phase: "Action",
  window,
  group,
  text: `The text of ${name}.`,
});
const catalog = {
  components: {
    action_cards: [
      card("mb1", "Morale Boost", "At the start of a combat round", "Combat"),
      card("mb2", "Morale Boost", "At the start of a combat round", "Combat"),
      card("mb3", "Morale Boost", "At the start of a combat round", "Combat"),
      card("rally", "Rally", "Action", "Action"),
      card(
        "sabo1",
        "Sabotage",
        "When another player plays an action card other than Sabotage",
        "Any time",
      ),
      card("scuttle", "Scuttle", "At the start of your turn", "Turns and actions"),
    ],
    action_card_groups: ["Action", "Combat", "Invasion", "Turns and actions", "Any time"],
  },
} as unknown as Catalog;
const withFacts: SessionUpdate = {
  ...menu,
  action_cards: { deck: 90, discard: ["mb2", "sabo1"], salvage: ["mb3"], hand_limit: 7 },
};
const NONE = { modes: {}, faked: [] };

it("shows the hand as printed, against the limit", () => {
  const view = selectActionCards(withFacts, catalog, { modes: { Scuttle: "never" }, faked: [] });
  expect(view).toMatchObject({ held: 2, limit: 7 });
  expect(view?.deck).toEqual({ left: 90, total: 6, discarded: 3 });
  expect(view?.hand).toEqual([
    // A component action is never offered in a window: it has no setting.
    {
      name: "Rally",
      window: "ACTION:",
      text: "The text of Rally.",
      action: true,
      group: "Action",
      copies: 1,
      offer: null,
    },
    {
      name: "Scuttle",
      window: "At the start of your turn:",
      text: "The text of Scuttle.",
      action: false,
      group: "Turns and actions",
      copies: 1,
      offer: { key: "Scuttle", name: "Scuttle", mode: "never", ownTurn: true },
    },
  ]);
});

it("counts the copies of a card by its name: discarded, held, and open", () => {
  const view = selectActionCards(withFacts, catalog, { modes: {}, faked: ["Sabotage", "Rally"] });
  const row = (name: string) =>
    view?.reference.flatMap((group) => group.cards).find((item) => item.name === name);
  // The groups are in the order of the catalogue; one with no card is left out.
  expect(
    view?.reference.map((group) => [group.group, group.cards.map((item) => item.name)]),
  ).toEqual([
    ["Action", ["Rally"]],
    ["Combat", ["Morale Boost"]],
    ["Turns and actions", ["Scuttle"]],
    ["Any time", ["Sabotage"]],
  ]);
  // A card on Garbozia is not in a hand either.
  expect(row("Morale Boost")).toMatchObject({ total: 3, discarded: 2, held: 0, open: 1 });
  expect(row("Sabotage")).toMatchObject({ total: 1, discarded: 1, open: 0, faked: true });
  // A card that is held is not faked.
  expect(row("Rally")).toMatchObject({ total: 1, held: 1, open: 0, faked: false });
  expect(view?.faked.map((item) => item.name)).toEqual(["Sabotage"]);
});

it("does not fake a component action", () => {
  // Seat a holds no Rally here: the setting is from before a card with "ACTION:" had no "Fake".
  const other = {
    ...withFacts,
    view: {
      ...withFacts.view,
      players: withFacts.view.players.map((player) => ({
        ...player,
        held_action_cards: ["scuttle"],
      })),
    },
  };
  const view = selectActionCards(other, catalog, { modes: {}, faked: ["Rally", "Morale Boost"] });
  const rally = view?.reference[0].cards[0];
  expect(rally).toMatchObject({ name: "Rally", action: true, held: 0, open: 1, faked: false });
  expect(view?.faked.map((item) => item.name)).toEqual(["Morale Boost"]);
});

it("has the hand by id without the catalogue, and no hand for a spectator", () => {
  const view = selectActionCards(menu, null, NONE);
  expect(view?.hand.map((item) => item.name)).toEqual(["Rally", "Scuttle"]);
  expect(view).toMatchObject({ limit: null, deck: null });
  expect(selectActionCards({ ...menu, viewer: { role: "spectator" } }, catalog, NONE)).toBeNull();
});
