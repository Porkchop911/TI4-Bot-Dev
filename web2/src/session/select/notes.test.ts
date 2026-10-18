import { describe, expect, it } from "vitest";
import type { Catalog, Choice, Phrase, PhraseHints, SessionUpdate } from "../wire";
import { optionWords, phraseWords, promptWords } from "./notes";

// A table of Sol (the viewer), Hacan and Jol-Nar, by their nicknames.
const update = {
  viewer: { role: "player", seat: "a" },
  seats: {
    a: { nickname: "Ann" },
    b: { nickname: "Bea" },
    c: { nickname: "Cid" },
  },
  view: {
    players: [
      { id: "a", faction: "sol" },
      { id: "b", faction: "hacan" },
      { id: "c", faction: "jolnar" },
    ],
  },
} as unknown as SessionUpdate;

const catalog = {
  components: {
    faction_sheets: { hacan: { name: "Hacan" }, jolnar: { name: "Jol-Nar" } },
  },
  records: {
    promissory_notes: [
      {
        id: "<color>_cf",
        name: "Ceasefire",
        text: "Play when <color> activates a system.",
      },
      {
        id: "ra",
        name: "Research Agreement",
        faction: "jolnar",
        text: "Gain a technology.",
      },
    ],
    action_cards: [
      {
        id: "bribery1",
        name: "Bribery",
        text: "Spend trade goods.",
        automation_id: "bribery",
      },
    ],
    secret_objectives: [],
  },
} as unknown as Catalog;

// Voidwatch of the Empyrean asks Hacan, and Kyver names the seat and the card in its prompt.
const voidwatch: Choice = {
  player: "b",
  prompt: "Voidwatch: which promissory note to give",
  options: [
    {
      id: "note|cf:hacan",
      kind: "promissory",
      label: "give cf:hacan to the Empyrean",
      payload: {},
    },
    {
      id: "note|ra:jolnar",
      kind: "promissory",
      label: "give ra:jolnar to the Empyrean",
      payload: {},
    },
  ],
  details: {
    phrases: {
      options: {
        "note|cf:hacan": {
          text: "give {note} to the Empyrean",
          refs: { note: { kind: "note", card: "<color>_cf", owner: "hacan" } },
        },
        "note|ra:jolnar": {
          text: "give {note} to the Empyrean",
          refs: { note: { kind: "note", card: "ra", owner: "jolnar" } },
        },
      },
    } satisfies PhraseHints,
  },
};
const kyver: Choice = {
  player: "a",
  prompt: "Kyver: c showed Bribery",
  options: [
    { id: "take", kind: "hero", label: "take Bribery from c", payload: {} },
    { id: "plain", kind: "hero", label: "something else", payload: {} },
  ],
  details: {
    phrases: {
      prompt: {
        text: "Kyver: {target} showed {card}",
        refs: {
          target: { kind: "seat", player: "c" },
          card: { kind: "action_card", card: "bribery" },
        },
      },
      options: {
        take: {
          text: "take {card} from {target}",
          refs: {
            target: { kind: "seat", player: "c" },
            card: { kind: "action_card", card: "bribery" },
          },
        },
      },
    } satisfies PhraseHints,
  },
};

describe("the texts that name ids (details.phrases)", () => {
  it("names a note as printed, with its owner when every faction has it, and its text for ?", () => {
    const [cf, ra] = voidwatch.options.map((option) =>
      optionWords(voidwatch, option, update, catalog),
    );
    expect(cf).toEqual({
      text: "Give Ceasefire (Hacan) to the Empyrean",
      help: [
        {
          title: "Ceasefire (Hacan)",
          text: "Play when Hacan activates a system.",
        },
      ],
    });
    expect(ra.text).toBe("Give Research Agreement to the Empyrean");
  });

  it("names a seat by its player and a card by its printed name, in the prompt too", () => {
    expect(promptWords(kyver, update, catalog)).toEqual({
      text: "Kyver: Cid showed Bribery",
      help: [{ title: "Bribery", text: "Spend trade goods." }],
    });
    expect(optionWords(kyver, kyver.options[0], update, catalog).text).toBe(
      "Take Bribery from Cid",
    );
  });

  it("shows a text with no phrase as the engine says it", () => {
    expect(optionWords(kyver, kyver.options[1], update, catalog)).toEqual({
      text: "Something else",
      help: [],
    });
    expect(promptWords(voidwatch, update, catalog).text).toBe(
      "Voidwatch: which promissory note to give",
    );
  });
});

// The subjects of the engine's phrases: Ann (Sol, the viewer) and Bea (Hacan), with Lodor (#27)
// and its planet Arnor on the map.
const subjectUpdate = {
  viewer: { role: "player", seat: "a" },
  seats: { a: { nickname: "Ann" }, b: { nickname: "Bea" } },
  view: {
    players: [
      { id: "a", faction: "sol" },
      { id: "b", faction: "hacan" },
    ],
    board: {
      map_tiles: [
        {
          system_id: "27",
          label: "Lodor",
          planets: [{ id: "arnor", label: "Arnor" }],
        },
      ],
    },
  },
} as unknown as SessionUpdate;

const subjectCatalog = {
  components: {
    faction_sheets: { hacan: { name: "Hacan" }, sol: { name: "Sol" } },
    agendas: [
      {
        id: "mutiny",
        name: "Mutiny",
        text: "For: each player who voted For gains 1 VP.",
      },
    ],
    strategy_cards: [{ id: "pok5trade", initiative: 5, name: "Trade" }],
    action_cards: [],
  },
  records: {
    promissory_notes: [],
    planets: [{ id: "jord", name: "Jord" }],
    units: [{ id: "cruiser2", name: "Cruiser II" }],
    action_cards: [],
    secret_objectives: [],
  },
} as unknown as Catalog;

const sayOf = (text: string, refs: Phrase["refs"]) =>
  phraseWords({ text, refs }, subjectUpdate, subjectCatalog).text;

describe("the subjects of the effect texts", () => {
  it("says the viewer's possessive as “your”, another seat's as its player's name", () => {
    expect(sayOf("{0} ships", { 0: { kind: "seat_possessive", player: "a" } })).toBe("your ships");
    expect(sayOf("{0} ships", { 0: { kind: "seat_possessive", player: "b" } })).toBe("Bea’s ships");
  });

  it("says a system as the map labels it, and a planet by its name on the map or as printed", () => {
    expect(sayOf("in {0}", { 0: { kind: "system", system: "27" } })).toBe("in Lodor · #27");
    expect(sayOf("in {0}", { 0: { kind: "system", system: "50" } })).toBe("in System · #50");
    expect(sayOf("on {0}", { 0: { kind: "planet", planet: "arnor" } })).toBe("on Arnor");
    expect(sayOf("on {0}", { 0: { kind: "planet", planet: "jord" } })).toBe("on Jord");
  });

  it("says units with their count, in the plural for a base unit, and a unit card as printed", () => {
    expect(sayOf("{0}", { 0: { kind: "units", unit_type: "cruiser", count: 2 } })).toBe(
      "2 Cruisers",
    );
    expect(sayOf("{0}", { 0: { kind: "units", unit_type: "war_sun", count: 1 } })).toBe(
      "1 War sun",
    );
    expect(sayOf("{0}", { 0: { kind: "units", unit_type: "cruiser2", count: 3 } })).toBe(
      "3 Cruiser II",
    );
    expect(sayOf("a {0}", { 0: { kind: "unit", unit_type: "dreadnought" } })).toBe("a Dreadnought");
  });

  it("says an agenda and a strategy card by their printed names, with the agenda's text for ?", () => {
    const words = phraseWords(
      {
        text: "Discards {0} and keeps {1}",
        refs: {
          0: { kind: "agenda", card: "mutiny" },
          1: { kind: "strategy_card", card: "pok5trade" },
        },
      },
      subjectUpdate,
      subjectCatalog,
    );
    expect(words.text).toBe("Discards Mutiny and keeps Trade");
    expect(words.help).toEqual([
      { title: "Mutiny", text: "For: each player who voted For gains 1 VP." },
    ]);
  });
});
