import { expect, it } from "vitest";
import menuUpdate from "../../../../fixtures/protocol/update-menu.json";
import type { Catalog, SessionUpdate } from "../wire";
import { selectAgendas } from "./agendas";

// No law is in play in this update of the real engine. Its facts are left out: a test gives its own.
const menu: SessionUpdate = { ...(menuUpdate as unknown as SessionUpdate), agendas: undefined };
const agenda = (
  id: string,
  name: string,
  kind: string,
  elect: string,
  group: string,
  against = false,
) => ({
  id,
  name,
  kind,
  elect,
  group,
  text: against ? `For: the text of *${name}*.` : `The text of *${name}*.`,
  ...(against && { against: `Against: the other text of ${name}.` }),
});
const catalog = {
  components: {
    agendas: [
      agenda("censure", "Political Censure", "Law", "Elect Player", "Victory points", false),
      agenda(
        "classified",
        "Classified Document Leaks",
        "Law",
        "Elect Scored Secret Objective",
        "Victory points",
      ),
      agenda("mutiny", "Mutiny", "Directive", "For/Against", "Victory points", true),
      agenda("regulations", "Fleet Regulations", "Law", "For/Against", "Supplies", true),
      agenda("sanctions", "Executive Sanctions", "Law", "For/Against", "Supplies", true),
      agenda("travel_ban", "Enforced Travel Ban", "Law", "For/Against", "Map", true),
      { ...agenda("covert", "Covert Legislation", "Directive", "", "Votes"), elect: undefined },
    ],
    agenda_groups: ["Victory points", "Planets", "Map", "Supplies", "Other"],
    secret_objectives: [
      { id: "btv", name: "Brave the Void", phase: "Action", group: "", text: "" },
    ],
  },
} as unknown as Catalog;
const withLaws = (laws: Record<string, string>, deck?: number): SessionUpdate => ({
  ...menu,
  view: { ...menu.view, table: { ...menu.view.table, laws } },
  ...(deck !== undefined && { agendas: { deck } }),
});

it("shows a law in play with the outcome that passed", () => {
  const view = selectAgendas(
    withLaws({ sanctions: "for", regulations: "against", censure: "d", classified: "btv" }),
    catalog,
  );
  expect(view.laws).toEqual([
    {
      name: "Classified Document Leaks",
      kind: "Law · Elect Scored Secret Objective",
      text: "The text of Classified Document Leaks.",
      outcome: { text: "Brave the Void" },
    },
    {
      name: "Executive Sanctions",
      kind: "Law · For/Against",
      text: "For: the text of Executive Sanctions.",
      outcome: { text: "For" },
    },
    {
      name: "Fleet Regulations",
      kind: "Law · For/Against",
      text: "Against: the other text of Fleet Regulations.",
      outcome: { text: "Against" },
    },
    {
      name: "Political Censure",
      kind: "Law · Elect Player",
      text: "The text of Political Censure.",
      outcome: { seat: "d" },
    },
  ]);
});

it("lists every other agenda once, in the groups of the catalogue", () => {
  const view = selectAgendas(withLaws({ sanctions: "for", censure: "d" }, 31), catalog);
  expect(view).toMatchObject({ rest: 5, deck: 31 });
  // A group with no agenda is left out; one that the catalogue does not list is last.
  expect(
    view.reference.map((group) => [group.group, group.cards.map((card) => card.name)]),
  ).toEqual([
    ["Victory points", ["Classified Document Leaks", "Mutiny"]],
    ["Map", ["Enforced Travel Ban"]],
    ["Supplies", ["Fleet Regulations"]],
    ["Votes", ["Covert Legislation"]],
  ]);
  const cards = view.reference.flatMap((group) => group.cards);
  // Both outcomes, one line each.
  expect(cards.find((card) => card.name === "Mutiny")).toEqual({
    name: "Mutiny",
    kind: "Directive · For/Against",
    text: "For: the text of Mutiny.\nAgainst: the other text of Mutiny.",
  });
  expect(cards.find((card) => card.name === "Covert Legislation")?.kind).toBe("Directive");
});

it("shows a law by its id without the catalogue, and no deck", () => {
  const view = selectAgendas(withLaws({ travel_ban: "for" }), null);
  expect(view).toEqual({
    laws: [{ name: "Travel Ban", kind: "", text: "", outcome: { text: "For" } }],
    rest: 0,
    deck: null,
    reference: [],
  });
  expect(selectAgendas(menu, catalog).laws).toEqual([]);
});
