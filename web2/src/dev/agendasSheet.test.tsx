import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it } from "vitest";
import { GameProvider } from "../game/context";
import { ReferenceDrawer } from "../game/shell/Reference";
import menuUpdate from "../session/fixtures/update-menu.json";
import { selectShell } from "../session/select/shell";
import type { Catalog, SessionUpdate } from "../session/wire";

// The Agendas sheet over an update of the real engine, with two laws put in play. The test is here
// because `dev` is the layer that may import both the session and the components.
const menu = menuUpdate as unknown as SessionUpdate;
const agenda = (id: string, name: string, kind: string, elect: string, group: string) => ({
  id,
  name,
  kind,
  elect,
  group,
  text: elect === "For/Against" ? `For: the text of ${name}.` : `The text of ${name}.`,
  ...(elect === "For/Against" && { against: `Against: the other text of ${name}.` }),
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
    secret_objectives: [],
    secret_objective_groups: [],
    agendas: [
      agenda(
        "censure",
        "Political Censure",
        "Law",
        "Elect Player",
        "Victory points and objectives",
      ),
      agenda("mutiny", "Mutiny", "Directive", "For/Against", "Victory points and objectives"),
      agenda("sanctions", "Executive Sanctions", "Law", "For/Against", "Laws and votes"),
      agenda("travel_ban", "Enforced Travel Ban", "Law", "For/Against", "Map and movement"),
    ],
    agenda_groups: ["Victory points and objectives", "Map and movement", "Laws and votes"],
  },
} as Catalog;
const update = (laws: Record<string, string>): SessionUpdate => ({
  ...menu,
  view: { ...menu.view, table: { ...menu.view.table, laws } },
  agendas: { deck: 40 },
});
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

function open(laws: Record<string, string>) {
  const view = selectShell(update(laws), local, catalog);
  render(
    <GameProvider seats={view.seats} dispatch={() => {}}>
      <ReferenceDrawer
        id="agendas"
        view={view.reference}
        players={view.players.rows}
        onClose={() => {}}
      />
    </GameProvider>,
  );
}

it("shows the laws in play as printed, with the outcome that passed", () => {
  open({ sanctions: "against", censure: "d" });
  const laws = within(screen.getByRole("region", { name: "Laws in play" }));
  expect(laws.getByText("2 laws")).toBeInTheDocument();
  expect(laws.getAllByRole("article").map((item) => item.getAttribute("aria-label"))).toEqual([
    "Executive Sanctions",
    "Political Censure",
  ]);
  const sanctions = within(laws.getByRole("article", { name: "Executive Sanctions" }));
  expect(sanctions.getByText("Law · For/Against")).toBeInTheDocument();
  expect(sanctions.getByText("Against", { exact: true })).toBeInTheDocument();
  // Only the outcome that passed.
  expect(
    sanctions.getByText("Against: the other text of Executive Sanctions."),
  ).toBeInTheDocument();
  expect(sanctions.queryByText(/^For:/)).not.toBeInTheDocument();
  // An elected seat is a tag: the symbol and the faction.
  const censure = laws.getByRole("article", { name: "Political Censure" });
  expect(censure.textContent).toContain("Xxcha");
});

it("says that no law is in play", () => {
  open({});
  const laws = within(screen.getByRole("region", { name: "Laws in play" }));
  expect(laws.getByText("0 laws")).toBeInTheDocument();
  expect(laws.getByText("None")).toBeInTheDocument();
});

it("lists the other agendas in groups by what an agenda affects", () => {
  open({ sanctions: "against", censure: "d" });
  const deck = within(screen.getByRole("region", { name: "Agenda deck" }));
  expect(deck.getByText("2 not in play · 40 in the deck")).toBeInTheDocument();
  // A law in play is not listed again, and a group with no agenda has no heading.
  expect(deck.getAllByRole("heading", { level: 4 }).map((item) => item.textContent)).toEqual([
    "▸Victory points and objectives1",
    "▸Map and movement1",
  ]);
  // A group is closed until the player opens it.
  expect(deck.queryByText("Mutiny")).not.toBeInTheDocument();
  const points = within(deck.getByRole("region", { name: "Victory points and objectives" }));
  fireEvent.click(points.getByRole("button"));
  expect(points.getByText("Mutiny")).toBeInTheDocument();
  expect(points.getByText("Directive · For/Against")).toBeInTheDocument();
  expect(
    points.getByText(/^For: the text of Mutiny\.\s+Against: the other text of Mutiny\.$/),
  ).toBeInTheDocument();
  expect(points.queryByText("Political Censure")).not.toBeInTheDocument();
  fireEvent.click(points.getByRole("button"));
  expect(points.queryByText("Mutiny")).not.toBeInTheDocument();
});
