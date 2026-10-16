import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it } from "vitest";
import { GameProvider } from "../game/context";
import { ReferenceDrawer } from "../game/shell/Reference";
import menuUpdate from "../session/fixtures/update-menu.json";
import { selectShell } from "../session/select/shell";
import type { Catalog, SessionUpdate } from "../session/wire";

// The Secrets sheet over an update of the real engine: seat a (Sol) holds `lsc` and `pe`. The test
// is here because `dev` is the layer that may import both the session and the components.
const secret = (id: string, name: string, phase: string, group: string) => ({
  id,
  name,
  phase,
  group,
  text: `The text of ${name}.`,
});
const catalog = {
  exploration: { cards: [], decks: {}, attachments: [] },
  components: {
    abilities: [],
    factions: {},
    leaders: [],
    technologies: [],
    relics: [],
    legendary: [],
    action_cards: [],
    action_card_groups: [],
    secret_objectives: [
      secret("btv", "Brave the Void", "Action", "Win a combat"),
      secret("ctr", "Control the Region", "Status", "Ships on the board"),
      secret("lsc", "Learn the Secrets of the Cosmos", "Status", "Ships on the board"),
      secret("pe", "Prove Endurance", "Action", "Agenda and table"),
      secret("te", "Threaten Enemies", "Status", "Ships on the board"),
    ],
    secret_objective_groups: ["Planets you control", "Ships on the board", "Win a combat"],
  },
} as Catalog;
const update: SessionUpdate = {
  ...(menuUpdate as unknown as SessionUpdate),
  secret_objectives: { deck: 25, scored: { a: ["btv"], d: ["te"] }, hand_limit: 3 },
};
const local = {
  inspected: null,
  staged: null,
  sent: null,
  error: null,
  replaying: null,
  canUndo: false,
  movement: {},
  landing: {},
  landingKind: null,
  production: { builds: [], planets: [], goods: 0 },
  productionStage: "build" as const,
  handled: [],
  step: null,
  remaining: null,
  planNote: null,
  stepping: null,
  draftLocked: null,
  draft: null,
};

function open() {
  const view = selectShell(update, local, catalog);
  render(
    <GameProvider seats={view.seats} dispatch={() => {}}>
      <ReferenceDrawer
        id="secrets"
        view={view.reference}
        players={view.players.rows}
        onClose={() => {}}
      />
    </GameProvider>,
  );
}

it("shows the secret objectives of the viewer as printed, and which are scored", () => {
  open();
  const mine = within(screen.getByRole("region", { name: "Your secret objectives" }));
  expect(mine.getByText("1 scored · 3 / 3")).toBeInTheDocument();
  expect(mine.getAllByRole("article").map((item) => item.getAttribute("aria-label"))).toEqual([
    "Learn the Secrets of the Cosmos",
    "Prove Endurance",
    "Brave the Void",
  ]);
  const held = within(mine.getByRole("article", { name: "Prove Endurance" }));
  expect(held.getByText("Not scored")).toBeInTheDocument();
  expect(held.getByText("Action phase")).toBeInTheDocument();
  expect(held.getByText("The text of Prove Endurance.")).toBeInTheDocument();
  expect(
    within(mine.getByRole("article", { name: "Brave the Void" })).getByText("✓ Scored"),
  ).toBeInTheDocument();
});

it("lists the deck in groups by what an objective asks for, with what is unseen", () => {
  open();
  const deck = within(screen.getByRole("region", { name: "Secret objective deck" }));
  expect(deck.getByText("1 of 5 unseen · 25 in the deck")).toBeInTheDocument();
  // A group with no objective has no heading; one that the catalogue does not list is last.
  expect(deck.getAllByRole("heading", { level: 4 }).map((item) => item.textContent)).toEqual([
    "▸Ships on the board1 of 3 unseen",
    "▸Win a combat0 of 1 unseen",
    "▸Agenda and table0 of 1 unseen",
  ]);
  // A group is closed until the player opens it.
  expect(deck.queryByText("Unseen")).not.toBeInTheDocument();
  const ships = within(deck.getByRole("region", { name: "Ships on the board" }));
  fireEvent.click(ships.getByRole("button"));
  expect(ships.getByText("Unseen")).toBeInTheDocument();
  expect(ships.getByText("In your hand")).toBeInTheDocument();
  // What another seat scored is public: the row names the seat.
  expect(ships.getByText(/Scored by/).textContent).toMatch(/^✓ Scored by.*Xxcha/);
  fireEvent.click(deck.getByRole("button", { name: /^Win a combat/ }));
  expect(deck.getByText("✓ Scored by you")).toBeInTheDocument();
  fireEvent.click(ships.getByRole("button"));
  expect(ships.queryByText("Unseen")).not.toBeInTheDocument();
});
