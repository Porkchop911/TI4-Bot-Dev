import { describe, expect, it } from "vitest";
import type { Catalog, Choice, PhraseHints, SessionUpdate } from "../wire";
import { optionWords, promptWords } from "./notes";

// A table of Sol (the viewer), Hacan and Jol-Nar, by their nicknames.
const update = {
  viewer: { role: "player", seat: "a" },
  seats: { a: { nickname: "Ann" }, b: { nickname: "Bea" }, c: { nickname: "Cid" } },
  view: {
    players: [
      { id: "a", faction: "sol" },
      { id: "b", faction: "hacan" },
      { id: "c", faction: "jolnar" },
    ],
  },
} as unknown as SessionUpdate;

const catalog = {
  components: { faction_sheets: { hacan: { name: "Hacan" }, jolnar: { name: "Jol-Nar" } } },
  records: {
    promissory_notes: [
      { id: "<color>_cf", name: "Ceasefire", text: "Play when <color> activates a system." },
      { id: "ra", name: "Research Agreement", faction: "jolnar", text: "Gain a technology." },
    ],
    action_cards: [
      { id: "bribery1", name: "Bribery", text: "Spend trade goods.", automation_id: "bribery" },
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
      help: [{ title: "Ceasefire (Hacan)", text: "Play when Hacan activates a system." }],
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
