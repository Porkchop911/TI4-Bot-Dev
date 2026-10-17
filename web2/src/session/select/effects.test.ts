import { describe, expect, it } from "vitest";
import type { Catalog, Choice, Phrase, SessionUpdate } from "../wire";
import { activeEffects, previewOf } from "./effects";
import { phraseWords } from "./notes";
import { reactedAction, selectReaction } from "./reaction";

// Ann (Sol, the viewer) and Bea (Hacan), with Lodor (#27) and its planet Arnor on the map.
const update = {
  viewer: { role: "player", seat: "a" },
  seats: { a: { nickname: "Ann" }, b: { nickname: "Bea" } },
  view: {
    phase: "action",
    players: [
      { id: "a", faction: "sol" },
      { id: "b", faction: "hacan" },
    ],
    board: {
      map_tiles: [{ system_id: "27", label: "Lodor", planets: [{ id: "arnor", label: "Arnor" }] }],
    },
    active_effects: [
      {
        seat: "a",
        card: "morale_boost1",
        effect: "combat_bonus",
        until: "end_of_combat_round",
        title: { text: "{0}", refs: { 0: { kind: "action_card", card: "morale_boost1" } } },
        summary: {
          text: "+1 to all combat rolls of {0} units",
          refs: { 0: { kind: "seat_possessive", player: "a" } },
        },
        expiry: "until the end of this combat round",
      },
      {
        seat: "b",
        effect: "cancel_hits",
        until: "end_of_combat",
        title: { text: "Shields Holding / Maneuvering Jets", refs: {} },
        summary: {
          text: "{0} hits cancelled",
          refs: { 0: { kind: "seat_possessive", player: "b" } },
        },
        expiry: "until the end of this combat",
      },
    ],
  },
} as unknown as SessionUpdate;

const catalog = {
  components: {
    faction_sheets: { hacan: { name: "Hacan" }, sol: { name: "Sol" } },
    agendas: [{ id: "mutiny", name: "Mutiny", text: "For: each player who voted For gains 1 VP." }],
    strategy_cards: [{ id: "pok5trade", initiative: 5, name: "Trade" }],
    action_cards: [],
  },
  records: {
    promissory_notes: [],
    planets: [{ id: "jord", name: "Jord" }],
    units: [{ id: "cruiser2", name: "Cruiser II" }],
    action_cards: [
      { id: "morale_boost1", name: "Morale Boost", text: "Apply +1 to each combat roll." },
      { id: "shields_holding1", name: "Shields Holding", text: "Cancel up to 2 hits." },
      { id: "sabotage2", name: "Sabotage", text: "Cancel 1 action card." },
      { id: "mining1", name: "Mining Initiative", text: "Gain trade goods." },
    ],
    secret_objectives: [],
  },
} as unknown as Catalog;

const say = (text: string, refs: Phrase["refs"]) =>
  phraseWords({ text, refs }, update, catalog).text;

describe("the subjects of the effect texts", () => {
  it("says the viewer's possessive as “your”, another seat's as its player's name", () => {
    expect(say("{0} ships", { 0: { kind: "seat_possessive", player: "a" } })).toBe("your ships");
    expect(say("{0} ships", { 0: { kind: "seat_possessive", player: "b" } })).toBe("Bea’s ships");
  });

  it("says a system as the map labels it, and a planet by its name on the map or as printed", () => {
    expect(say("in {0}", { 0: { kind: "system", system: "27" } })).toBe("in Lodor · #27");
    expect(say("in {0}", { 0: { kind: "system", system: "50" } })).toBe("in System · #50");
    expect(say("on {0}", { 0: { kind: "planet", planet: "arnor" } })).toBe("on Arnor");
    expect(say("on {0}", { 0: { kind: "planet", planet: "jord" } })).toBe("on Jord");
  });

  it("says units with their count, in the plural for a base unit, and a unit card as printed", () => {
    expect(say("{0}", { 0: { kind: "units", unit_type: "cruiser", count: 2 } })).toBe("2 Cruisers");
    expect(say("{0}", { 0: { kind: "units", unit_type: "war_sun", count: 1 } })).toBe("1 War sun");
    expect(say("{0}", { 0: { kind: "units", unit_type: "cruiser2", count: 3 } })).toBe(
      "3 Cruiser II",
    );
    expect(say("a {0}", { 0: { kind: "unit", unit_type: "dreadnought" } })).toBe("a Dreadnought");
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
      update,
      catalog,
    );
    expect(words.text).toBe("Discards Mutiny and keeps Trade");
    expect(words.help).toEqual([
      { title: "Mutiny", text: "For: each player who voted For gains 1 VP." },
    ]);
  });
});

// Bea plays Mining Initiative; Ann may play Sabotage or Shields Holding, the engine words both.
const window: Choice = {
  player: "a",
  prompt: "Reaction",
  options: [
    { id: "ac|0", kind: "action_card", label: "play Sabotage", payload: {} },
    { id: "ac|1", kind: "action_card", label: "play Shields Holding", payload: {} },
    { id: "pass", kind: "decline", label: "pass", payload: {} },
  ],
  context: {
    previews: [
      {
        option: "ac|0",
        card: "sabotage2",
        effect: "cancel_card",
        phrase: {
          text: "cancels {0} {1}. The card stays spent and its effect does not happen.",
          refs: {
            0: { kind: "seat_possessive", player: "b" },
            1: { kind: "action_card", card: "mining1" },
          },
        },
      },
      {
        option: "ac|1",
        card: "shields_holding1",
        effect: "cancel_hits",
        phrase: {
          text: "Cancels 2 of the 5 hits on {0} ships in {1}, leaving 3.",
          refs: {
            0: { kind: "seat_possessive", player: "a" },
            1: { kind: "system", system: "27" },
          },
        },
      },
    ],
  },
} as unknown as Choice;

const reacting = {
  ...update,
  reaction: {
    trigger: {
      kind: "action_card_played",
      event_type: "ACTION_CARD_PLAYED",
      relation: "when",
      actor: "b",
      card: "mining1",
    },
    own_turn_applies: false,
    offers: [
      { option: "ac|0", key: "Sabotage", kind: "action_card", name: "Sabotage", text: "Cancel." },
      { option: "ac|1", key: "Shields Holding", kind: "action_card", name: "Shields Holding" },
    ],
  },
} as unknown as SessionUpdate;

describe("what a reaction would do now (the previews of the engine)", () => {
  it("shows the sentence of the engine on the row of its option, with the card it names for ?", () => {
    const view = selectReaction(reacting, window, null, catalog, {})!;
    expect(view.rows.map((row) => row.effect)).toEqual([
      "Cancels Bea’s Mining Initiative. The card stays spent and its effect does not happen.",
      "Cancels 2 of the 5 hits on your ships in Lodor · #27, leaving 3.",
    ]);
    expect(view.rows[0].help).toEqual([{ title: "Mining Initiative", text: "Gain trade goods." }]);
    expect(view.rows[0].text).toBe("Cancel.");
  });

  it("has no sentence for an option without a preview", () => {
    expect(previewOf(window, "pass", reacting, catalog)).toBeNull();
  });

  it("says what a played card will do from the pending effect of that copy, else not resolved yet", () => {
    const trigger = reacting.reaction!.trigger;
    const note = (choice: Choice) =>
      reactedAction(reacting, trigger, catalog, choice).blocks.find(
        (block) => block.kind === "note",
      );
    expect(note(window)).toMatchObject({ text: "Not resolved yet." });
    const pending = (card: string) =>
      ({
        ...window,
        context: {
          pending_effect: {
            option: "",
            card,
            effect: "take_commodities",
            amount: 3,
            phrase: {
              text: "{0} would gain 3 commodities, as trade goods.",
              refs: { 0: { kind: "seat", player: "b" } },
            },
          },
        },
      }) as unknown as Choice;
    expect(note(pending("mining1"))).toMatchObject({
      text: "Bea would gain 3 commodities, as trade goods.",
    });
    // The pending effect of another copy is not this card's.
    expect(note(pending("mining2"))).toMatchObject({ text: "Not resolved yet." });
    // A preview of an offered card is never the played card's result.
    const offered = {
      ...window,
      context: {
        previews: [
          {
            option: "ac|9",
            card: "mining1",
            effect: "cancel_card",
            phrase: { text: "Cancels it.", refs: {} },
          },
        ],
      },
    } as unknown as Choice;
    expect(note(offered)).toMatchObject({ text: "Not resolved yet." });
  });
});

describe("the lingering effects in force", () => {
  it("has the card, what it does and until when, with the printed text of the card for ?", () => {
    expect(activeEffects(update, catalog)).toEqual([
      {
        title: "Morale Boost",
        summary: "+1 to all combat rolls of your units",
        expiry: "until the end of this combat round",
        help: [{ title: "Morale Boost", text: "Apply +1 to each combat roll." }],
      },
      {
        title: "Shields Holding / Maneuvering Jets",
        summary: "Bea’s hits cancelled",
        expiry: "until the end of this combat",
        help: [],
      },
    ]);
  });
});
