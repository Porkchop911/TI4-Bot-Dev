import { describe, expect, it } from "vitest";
import type { AgendaPlacementHints, Catalog, Choice } from "../wire";
import { clearLastAgenda, placeAgenda, selectAgendaDeck } from "./agendaDeck";

// The first question of Politics' placement as the engine asks it (the scenario
// "strategy_politics"): the agenda of this question, and every agenda to place.
const first: Choice = {
  player: "a",
  prompt: "place minister_industry where",
  options: [
    { id: "top", kind: "agenda", label: "on top of the deck", payload: {} },
    { id: "bottom", kind: "agenda", label: "on the bottom", payload: {} },
  ],
  details: {
    agenda: {
      id: "minister_industry",
      name: "Minister of Industry",
      type: "Law",
      target: "Elect Player",
      text1: "The elected player gains this card.",
      text2: "",
    },
    ...({ agendas: ["minister_industry", "imperial_arbiter"] } satisfies AgendaPlacementHints),
  },
};
const printed = {
  components: {
    agendas: [
      {
        id: "minister_industry",
        name: "Minister of Industry",
        kind: "Law",
        elect: "Elect Player",
        text: "The elected player gains this card. *PRODUCTION*",
        group: "Laws",
      },
      {
        id: "imperial_arbiter",
        name: "Imperial Arbiter",
        kind: "Law",
        elect: "Elect Player",
        text: "The elected player gains this card.",
        group: "Laws",
      },
    ],
  },
} as unknown as Catalog;

const place = (draft: ReturnType<typeof placeAgenda>, agenda: string, where: string) =>
  placeAgenda(first, draft, `agenda:${agenda}`, where);

describe("the placement of agenda cards", () => {
  it("is every agenda as printed, with top and bottom, and nothing to send yet", () => {
    const view = selectAgendaDeck(first, null, printed)!;
    expect(view.blocks.filter((block) => block.kind === "source")).toEqual([
      {
        kind: "source",
        name: "Minister of Industry",
        type: "Agenda · Law · Elect Player",
        text: "The elected player gains this card. PRODUCTION",
      },
      {
        kind: "source",
        name: "Imperial Arbiter",
        type: "Agenda · Law · Elect Player",
        text: "The elected player gains this card.",
      },
    ]);
    const keys = view.blocks.flatMap((block) =>
      block.kind === "menu" ? block.rows.map((row) => `${row.key} ${row.title}`) : [],
    );
    expect(keys).toEqual([
      "1 Top of the deck",
      "2 Bottom of the deck",
      "3 Top of the deck",
      "4 Bottom of the deck",
    ]);
    expect(view.problem).toBe("Place the 2 agenda cards.");
    expect(view.answers).toBeNull();
    expect(view.label).toBe("Place 2 agenda cards");
  });

  it("shows the deck as the game builds it and answers in the order that the game asks", () => {
    let draft = place(null, "imperial_arbiter", "top");
    expect(selectAgendaDeck(first, draft, printed)!.problem).toBe("Place 1 more agenda card.");
    draft = place(draft, "minister_industry", "top");
    const view = selectAgendaDeck(first, draft, printed)!;
    const deck = view.blocks.find((block) => block.kind === "card");
    // The agenda asked second goes over the one asked first.
    expect(deck?.kind === "card" && deck.rows.map((row) => row.subtitle)).toEqual([
      "Imperial Arbiter",
      "Minister of Industry",
    ]);
    expect(deck?.kind === "card" && deck.aside).toBe("2 / 2 placed");
    expect(view.answers).toEqual(["top", "top"]);
    draft = place(draft, "minister_industry", "bottom");
    expect(selectAgendaDeck(first, draft, printed)!.answers).toEqual(["bottom", "top"]);
  });

  it("takes a place back when it is chosen again; Esc clears the last place", () => {
    let draft = place(null, "minister_industry", "top");
    expect(place(draft, "minister_industry", "top")).toEqual({ placed: [] });
    draft = place(draft, "imperial_arbiter", "bottom");
    expect(clearLastAgenda(draft)).toEqual({
      placed: [{ agenda: "minister_industry", place: "top" }],
    });
    expect(place(draft, "other", "top")).toBeNull();
    expect(placeAgenda(first, draft, undefined, "top")).toBeNull();
  });

  it("is not another question, and prints the agenda of the details without a catalogue", () => {
    const yesNo: Choice = {
      ...first,
      options: [
        { id: "yes", kind: "strategy", label: "yes", payload: {} },
        { id: "no", kind: "strategy", label: "no", payload: {} },
      ],
    };
    expect(selectAgendaDeck(yesNo, null, printed)).toBeNull();
    const view = selectAgendaDeck(first, null, null)!;
    expect(view.blocks[0]).toEqual({
      kind: "source",
      name: "Minister of Industry",
      type: "Agenda · Law",
      text: "The elected player gains this card.",
    });
  });
});
