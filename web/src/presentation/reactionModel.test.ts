import { describe, expect, it } from "vitest";
import type { GameEvent, PendingChoiceDto } from "../protocol/types.ts";
import { describeReaction } from "./reactionModel.ts";
import { partialContext } from "../test/wire.ts";

const labels = {
  playerLabel: (id: string) => ({ p1: "Anna", p2: "Bob" })[id] ?? id,
};

const sabotageOffer = (subtype = "reaction_when_ACTION_CARD_PLAYED"): PendingChoiceDto => ({
  actor: "p2",
  nonce: "1",
  prompt: "when ACTION_CARD_PLAYED",
  context: partialContext({ subtype, optional: true, source: { Reaction: "ACTION_CARD_PLAYED" } }),
  options: [
    {
      id: "reaction:Sol:ACTION_CARD_PLAYED:when",
      kind: "ability",
      label: "Play Sabotage",
      payload: { card: "sabo1", card_name: "Sabotage" },
    },
    { id: "decline", kind: "decline", label: "Pass", payload: {} },
  ],
});

const logEntry = (actor: string, detail: string): GameEvent => ({
  visibility: { visibility: "public" as const },
  id: "e1",
  timestamp: "t",
  actor,
  detail,
  event: { kind: "decision_resolved" },
});

describe("describeReaction", () => {
  it("builds the button from the card name, never 'Play play'", () => {
    const inner = sabotageOffer("play_reaction_when_ACTION_CARD_PLAYED");
    inner.options[0] = {
      id: "sabo1",
      kind: "action_card",
      label: "play Sabotage",
      payload: { card: "sabo1", card_name: "Sabotage" },
    };
    const model = describeReaction({ choice: inner, ...labels, viewerSeat: "p2" });
    expect(model.reactions.map((row) => row.buttonLabel)).toEqual(["Play Sabotage"]);
    expect(model.reactions[0].card?.text).toBe("Cancel that action card.");
    expect(model.reactions[0].card?.window).toContain("When another player plays an action card");
    const outer = describeReaction({ choice: sabotageOffer(), ...labels, viewerSeat: "p2" });
    expect(outer.reactions[0].buttonLabel).toBe("Play Sabotage");
    expect(outer.declineOptionId).toBe("decline");
  });

  it("level 2: names the card player from the public log", () => {
    const model = describeReaction({
      choice: sabotageOffer(),
      events: [logEntry("p1", "p1 played Mining Initiative")],
      ...labels,
      viewerSeat: "p2",
    });
    expect(model.source).toBe("log");
    expect(model.sentence).toBe("Anna played the action card Mining Initiative.");
    expect(model.facts.card?.text).toMatch(/\S/);
    expect(model.canNowSentence).toBe("Before this resolves, you can play Sabotage.");
    expect(model.title).toBe("Anna played an action card");
  });

  it("level 2: names the active player and system for an activation", () => {
    const choice = sabotageOffer("reaction_after_SYSTEM_ACTIVATED");
    const model = describeReaction({
      choice,
      activePlayerId: "p1",
      activeSystemId: "27",
      ...labels,
      viewerSeat: "p2",
    });
    expect(model.sentence).toBe("Anna activated System 27.");
    expect(model.canNowSentence).toBe("Now you can play Sabotage.");
  });

  it("level 3: only the window, never a raw engine id", () => {
    const model = describeReaction({
      choice: sabotageOffer("reaction_after_SHIP_DESTROYED"),
      ...labels,
      viewerSeat: "p2",
    });
    expect(model.source).toBe("subtype");
    expect(model.sentence).toBe("A reaction window opened: after a ship is destroyed.");
    expect(`${model.sentence}${model.title}`).not.toMatch(/[A-Z]{3,}_[A-Z_]+/);
  });

  it("never names the viewer as the trigger actor of someone else's card", () => {
    const model = describeReaction({
      choice: sabotageOffer(),
      events: [logEntry("p2", "p2 played Sabotage")],
      ...labels,
      viewerSeat: "p2",
    });
    expect(model.source).toBe("subtype");
  });

  it("turns Instinct Training into a use step with its text", () => {
    const model = describeReaction({
      choice: {
        actor: "p2",
        nonce: "2",
        prompt: "Instinct Training: exhaust ... cancel sabo1",
        context: partialContext({ subtype: "instinct_training_cancel", source: { Content: "it" } }),
        options: [
          { id: "use", kind: "technology", label: "cancel it", payload: {} },
          { id: "decline", kind: "decline", label: "Pass", payload: {} },
        ],
      },
      ...labels,
      viewerSeat: "p2",
    });
    expect(model.eventType).toBe("ACTION_CARD_PLAYED");
    expect(model.reactions[0].buttonLabel).toBe("Use Instinct Training");
    expect(model.reactions[0].note).toMatch(/cancel/i);
  });

  it("names faction notes, unit cards and leaders from the catalog instead of their engine ids", () => {
    const offer = (id: string): PendingChoiceDto => ({
      actor: "p2",
      nonce: "3",
      prompt: "when SYSTEM_ACTIVATED",
      context: partialContext({ subtype: "reaction_after_SYSTEM_ACTIVATED", optional: true, source: { Reaction: "SYSTEM_ACTIVATED" } }),
      options: [
        { id, kind: "ability", label: `Play ${id}`, payload: {} },
        { id: "decline", kind: "decline", label: "Pass", payload: {} },
      ],
    });
    const label = (id: string) =>
      describeReaction({ choice: offer(id), ...labels, viewerSeat: "p2" }).reactions[0];
    expect(label("promissory:empyrean:iff:TURN_BEGAN:after").buttonLabel).toBe("Use Creuss Iff");
    expect(label("promissory:empyrean:iff:TURN_BEGAN:after").note).toMatch(/wormhole/i);
    expect(label("unit:arborec:arborec_flagship:SYSTEM_ACTIVATED:after").buttonLabel).toBe("Use Duha Menaimon");
    expect(label("leader:argent:argentcommander:UNIT_ABILITY_ROLLED:").buttonLabel).toBe("Use Trrakan Aun Zulok");
    expect(label("promissory:xxcha:cf:SYSTEM_ACTIVATED:after").buttonLabel).toBe("Use Ceasefire");
  });

  it("keeps the engine label for an unnamed multi-card outer offer", () => {
    const choice = sabotageOffer();
    choice.options[0] = { id: "reaction:Sol:X:when", kind: "ability", label: "Choose an action card…", payload: {} };
    const model = describeReaction({ choice, ...labels, viewerSeat: "p2" });
    expect(model.reactions[0].buttonLabel).toBe("Choose an action card…");
  });

  const triggered = (
    subtype: string,
    trigger: NonNullable<PendingChoiceDto["context"]>["trigger"],
  ): PendingChoiceDto => {
    const choice = sabotageOffer(subtype);
    choice.context = { ...choice.context!, trigger };
    return choice;
  };

  it("level 1: the typed trigger wins over the log and says it verbatim (user example 1)", () => {
    const model = describeReaction({
      choice: triggered("reaction_when_ACTION_CARD_PLAYED", {
        kind: "action_card_played",
        event_type: "ACTION_CARD_PLAYED",
        event_id: 4,
        relation: "when",
        actor: "p1",
        card: "uprising",
      }),
      // A stale log entry must not override the trigger.
      events: [logEntry("p1", "p1 played Plague")],
      ...labels,
      viewerSeat: "p2",
    });
    expect(model.source).toBe("trigger");
    expect(model.sentence).toBe("Anna played the action card Uprising.");
    expect(model.facts.card?.text).toMatch(/^Exhaust 1 non-home planet/);
    expect(model.canNowSentence).toBe("Before this resolves, you can play Sabotage.");
  });

  it("level 1: a system activation names the system and, after, says 'Now you can' (example 2)", () => {
    const model = describeReaction({
      choice: triggered("reaction_after_SYSTEM_ACTIVATED", {
        kind: "system_activated",
        event_type: "SYSTEM_ACTIVATED",
        event_id: 5,
        relation: "after",
        actor: "p1",
        system: "27",
      }),
      ...labels,
      systemLabel: (id) => `System ${id} (Lodor)`,
      viewerSeat: "p2",
    });
    expect(model.sentence).toBe("Anna activated System 27 (Lodor).");
    expect(model.canNowSentence).toBe("Now you can play Sabotage.");
    expect(model.facts.systemId).toBe("27");
  });

  it("says You when the viewer is the actor and lists moved ships", () => {
    const model = describeReaction({
      choice: triggered("reaction_after_SHIP_MOVED", {
        kind: "ship_moved",
        event_type: "SHIP_MOVED",
        event_id: 6,
        relation: "after",
        actor: "p2",
        system: "27",
        units: [
          { owner: "p2", unit_type: "cruiser", count: 2 },
          { owner: "p2", unit_type: "dreadnought", count: 1 },
        ],
      }),
      ...labels,
      viewerSeat: "p2",
    });
    expect(model.sentence).toBe("You moved ships into System 27: 2 cruisers, 1 dreadnought.");
  });

  it("has no actor for an agenda reveal and uses the strategy card's name", () => {
    const agenda = describeReaction({
      choice: triggered("reaction_when_AGENDA_REVEALED", {
        kind: "agenda_revealed",
        event_type: "AGENDA_REVEALED",
        event_id: 7,
        relation: "when",
        agenda: "mutiny",
      }),
      ...labels,
      viewerSeat: "p2",
    });
    expect(agenda.sentence).toBe("The agenda Mutiny was revealed.");
    expect(agenda.facts.actorId).toBeNull();
    const strategic = describeReaction({
      choice: triggered("reaction_when_STRATEGIC_ACTION_BEGAN", {
        kind: "strategic_action_began",
        event_type: "STRATEGIC_ACTION_BEGAN",
        event_id: 8,
        relation: "when",
        actor: "p1",
        card: "pok1leadership",
      }),
      ...labels,
      viewerSeat: "p2",
    });
    expect(strategic.sentence).toBe("Anna is about to use the Leadership strategy card.");
  });

  it("marks a reaction to a reaction and notes how many of your units stand in the system", () => {
    const model = describeReaction({
      choice: triggered("reaction_after_SYSTEM_ACTIVATED", {
        kind: "system_activated",
        event_type: "SYSTEM_ACTIVATED",
        event_id: 9,
        relation: "after",
        actor: "p1",
        system: "27",
        chain: [3],
      }),
      board: {
        systems: {
          "27": {
            system_id: "27",
            command_tokens: [],
            planets: {},
            units: [
              { unit_type: "cruiser", owner: "p2", damaged: false, planet: null, galvanized: false },
              { unit_type: "cruiser", owner: "p2", damaged: false, planet: null, galvanized: false },
              { unit_type: "cruiser", owner: "p1", damaged: false, planet: null, galvanized: false },
            ],
          },
        },
      },
      ...labels,
      viewerSeat: "p2",
    });
    expect(model.inResponse).toBe(true);
    expect(model.note).toBe("You have 2 units there.");
  });

  it("falls back to the log when a malformed trigger arrives", () => {
    const choice = sabotageOffer();
    choice.context = { ...choice.context!, trigger: { nonsense: true } as never };
    const model = describeReaction({
      choice,
      events: [logEntry("p1", "p1 played Plague")],
      ...labels,
      viewerSeat: "p2",
    });
    expect(model.source).toBe("log");
    expect(model.sentence).toBe("Anna played the action card Plague.");
  });
});

describe("Salvage effect", () => {
  const salvageOffer = (): PendingChoiceDto => ({
    actor: "p1",
    nonce: "1",
    prompt: "after SPACE_COMBAT_WON",
    context: partialContext({
      subtype: "reaction_after_SPACE_COMBAT_WON",
      optional: true,
      trigger: {
        kind: "space_combat_won",
        event_type: "SPACE_COMBAT_WON",
        event_id: 7,
        relation: "after",
        actor: "p1",
        system: "27",
      },
    }),
    options: [
      {
        id: "reaction:Sol:SPACE_COMBAT_WON:after",
        kind: "ability",
        label: "Play Salvage",
        payload: { card: "salvage", card_name: "Salvage" },
      },
      { id: "decline", kind: "decline", label: "Pass", payload: {} },
    ],
  });
  const board = {
    systems: {},
    combat: { system_id: "27", round: 2, attacker: "p1", defender: "p2" },
  } as never;

  it("says how many commodities the opponent would hand over", () => {
    const model = describeReaction({
      choice: salvageOffer(),
      ...labels,
      viewerSeat: "p1",
      board,
      commodities: (seat) => ({ p1: 1, p2: 3 })[seat] ?? null,
    });
    expect(model.reactions[0].effect).toBe("You would gain 3 commodities from Bob, as trade goods.");
  });

  it("says so when there is nothing to take, and stays silent when the opponent is unknown", () => {
    const empty = describeReaction({
      choice: salvageOffer(),
      ...labels,
      viewerSeat: "p1",
      board,
      commodities: () => 0,
    });
    expect(empty.reactions[0].effect).toContain("would gain nothing");
    const unknown = describeReaction({ choice: salvageOffer(), ...labels, viewerSeat: "p1" });
    expect(unknown.reactions[0].effect).toBeNull();
  });
});

describe("Engine effect previews", () => {
  const sabotageOffer = (previews: unknown): PendingChoiceDto => ({
    actor: "p1",
    nonce: "1",
    prompt: "when ACTION_CARD_PLAYED",
    context: partialContext({
      subtype: "reaction_when_ACTION_CARD_PLAYED",
      optional: true,
      trigger: {
        kind: "action_card_played",
        event_type: "ACTION_CARD_PLAYED",
        event_id: 7,
        relation: "when",
        actor: "p2",
        card: "fs1",
      },
      previews: previews as never,
    }),
    options: [
      {
        id: "reaction:Sol:ACTION_CARD_PLAYED:when",
        kind: "ability",
        label: "Play Sabotage",
        payload: { card: "sabo1", card_name: "Sabotage" },
      },
      { id: "decline", kind: "decline", label: "Pass", payload: {} },
    ],
  });

  it("words the preview of the option as the row's effect", () => {
    const model = describeReaction({
      choice: sabotageOffer([
        {
          option: "reaction:Sol:ACTION_CARD_PLAYED:when",
          card: "sabo1",
          effect: "cancel_card",
          seats: ["p2"],
          refs: ["fs1"],
          phrase: {
            text: "Cancels {0} {1}. The card stays spent and its effect does not happen.",
            refs: {
              0: { kind: "seat_possessive", player: "p2" },
              1: { kind: "action_card", card: "fs1" },
            },
          },
        },
      ]),
      ...labels,
      viewerSeat: "p1",
    });
    expect(model.reactions[0].effect).toBe(
      "Cancels Bob's Flank Speed. The card stays spent and its effect does not happen.",
    );
  });

  it("leaves the effect empty for an older server, a damaged preview or one without words", () => {
    for (const previews of [undefined, "junk", [{ option: 3 }], [{ option: "reaction:Sol:ACTION_CARD_PLAYED:when", card: "x", effect: "later" }]]) {
      const model = describeReaction({ choice: sabotageOffer(previews), ...labels, viewerSeat: "p1" });
      expect(model.reactions[0].effect).toBeNull();
    }
  });
});
