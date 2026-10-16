import { describe, it, expect } from "vitest";
import {
  deriveChoiceRendererModel,
  getPaymentPayload,
  getMovementPayload,
  getTradePayload,
  getCombatPayload,
} from "./choiceModel.ts";
import { PendingChoiceDto } from "../protocol/types.ts";
import { partialContext } from "../test/wire.ts";

describe("choiceModel", () => {
  const baseChoice: PendingChoiceDto = {
    nonce: "n1",
    actor: "seat_1",
    prompt: "Make a choice",
    options: [
      { id: "opt1", label: "Option 1", kind: "default", payload: {} },
      { id: "decline", label: "Decline", kind: "decline", payload: {} },
    ],
  };

  describe("deriveChoiceRendererModel classification", () => {
    it("returns null if choice is null or viewer is not actor", () => {
      expect(deriveChoiceRendererModel(null, "seat_1")).toBeNull();
      expect(deriveChoiceRendererModel(baseChoice, null)).toBeNull();
      expect(deriveChoiceRendererModel(baseChoice, "seat_2")).toBeNull();
    });

    it("classifies payment workflows and extracts owed/paid amounts", () => {
      const choice: PendingChoiceDto = {
        ...baseChoice,
        prompt: "pay 4 more resources",
        context: partialContext({
          subtype: "pay_resources",
          outstanding: [{ kind: "Resources", amount: 6, paid: 2 }],
        }),
        options: [
          {
            id: "exhaust|jord",
            label: "Jord",
            kind: "pay",
            payload: { worth: 4, owed: 4, kind: "resources" },
          },
          {
            id: "trade_good",
            label: "Trade Good",
            kind: "pay",
            payload: { worth: 1, owed: 4, kind: "resources" },
          },
        ],
      };

      const model = deriveChoiceRendererModel(choice, "seat_1");
      expect(model).not.toBeNull();
      expect(model?.workflow).toBe("payment");
      expect(model?.selectionMode).toEqual({
        mode: "quantity",
        target: 6,
        paid: 2,
        unit: "resources",
      });
    });

    it("classifies influence payment with correct currency unit", () => {
      const choice: PendingChoiceDto = {
        ...baseChoice,
        prompt: "pay 3 influence",
        context: partialContext({
          subtype: "pay_influence",
          outstanding: [{ kind: "Influence", amount: 3, paid: 0 }],
        }),
      };

      const model = deriveChoiceRendererModel(choice, "seat_1");
      expect(model?.workflow).toBe("payment");
      expect(model?.selectionMode).toEqual({
        mode: "quantity",
        target: 3,
        paid: 0,
        unit: "influence",
      });
    });

    it("does NOT classify remove_custodians as payment (falls through to standard selection)", () => {
      const choice: PendingChoiceDto = {
        ...baseChoice,
        prompt: "spend 6 influence to remove custodians?",
        context: partialContext({
          subtype: "remove_custodians",
        }),
        options: [
          { id: "yes", label: "Yes", kind: "custodians", payload: {} },
          { id: "no", label: "No", kind: "decline", payload: {} },
        ],
      };

      const model = deriveChoiceRendererModel(choice, "seat_1");
      expect(model?.workflow).toBe("generic_selection");
      expect(model?.selectionMode).toEqual({ mode: "single" });
    });

    it("classifies system activation", () => {
      const choice: PendingChoiceDto = {
        ...baseChoice,
        prompt: "activate a system",
        context: partialContext({ subtype: "activate_system" }),
        options: [
          { id: "18", label: "Mecatol Rex", kind: "activate", payload: { system: "18" } },
          { id: "24", label: "Moll Primus", kind: "activate", payload: { system: "24" } },
        ],
      };

      const model = deriveChoiceRendererModel(choice, "seat_1");
      expect(model?.workflow).toBe("system_activation");
      expect(model?.selectionMode).toEqual({ mode: "single" });
      expect(model?.optionsByTarget.get("18")).toHaveLength(1);
    });

    it("classifies tactical movement, cargo, and invasion", () => {
      const moveChoice: PendingChoiceDto = {
        ...baseChoice,
        prompt: "movement",
        context: partialContext({
          subtype: "movement_step",
          target: { System: "18" },
        }),
        options: [
          {
            id: "move|24|0",
            label: "Cruiser",
            kind: "move",
            payload: { origin: "24", unit: "cruiser" },
          },
          { id: "done_moving", label: "Finish", kind: "decline", payload: {} },
        ],
      };

      const moveModel = deriveChoiceRendererModel(moveChoice, "seat_1");
      expect(moveModel?.workflow).toBe("tactical_movement");
      expect(moveModel?.selectionMode).toEqual({ mode: "tactical_move", activeSystem: "18" });

      const cargoChoice: PendingChoiceDto = {
        ...baseChoice,
        prompt: "load cargo",
        context: partialContext({ subtype: "load_cargo", target: { System: "18" } }),
      };
      expect(deriveChoiceRendererModel(cargoChoice, "seat_1")?.workflow).toBe("tactical_cargo");

      const invasionChoice: PendingChoiceDto = {
        ...baseChoice,
        prompt: "commit ground forces",
        context: partialContext({ subtype: "commit_ground_forces", target: { System: "18" } }),
      };
      expect(deriveChoiceRendererModel(invasionChoice, "seat_1")?.workflow).toBe(
        "tactical_invasion",
      );
    });

    it("classifies production builder", () => {
      const choice: PendingChoiceDto = {
        ...baseChoice,
        prompt: "produce units",
        context: partialContext({
          subtype: "produce_unit",
          target: { System: "18" },
          outstanding: [{ kind: "ProductionCapacity", amount: 5 }],
        }),
        options: [
          { id: "build|fighter", label: "Fighter", kind: "produce", payload: {} },
          { id: "decline", label: "Done", kind: "decline", payload: {} },
        ],
      };

      const model = deriveChoiceRendererModel(choice, "seat_1");
      expect(model?.workflow).toBe("production");
      expect(model?.selectionMode).toEqual({
        mode: "production",
        capacity: 5,
        systemId: "18",
      });
      expect(model?.isOptional).toBe(true);
    });

    it("classifies public and secret objective scoring", () => {
      const publicObjective: PendingChoiceDto = {
        ...baseChoice,
        context: partialContext({ subtype: "score_objective" }),
      };
      const secretObjective: PendingChoiceDto = {
        ...baseChoice,
        context: partialContext({ subtype: "score_secret_objective" }),
      };

      expect(deriveChoiceRendererModel(publicObjective, "seat_1")?.workflow).toBe(
        "objective_scoring",
      );
      expect(deriveChoiceRendererModel(secretObjective, "seat_1")?.workflow).toBe(
        "objective_scoring",
      );
    });

    it("classifies combat sustain and casualty assignment", () => {
      const sustainChoice: PendingChoiceDto = {
        ...baseChoice,
        prompt: "sustain damage",
        context: partialContext({
          subtype: "sustain_damage",
          outstanding: [{ amount: 2 }],
        }),
      };
      const sustainModel = deriveChoiceRendererModel(sustainChoice, "seat_1");
      expect(sustainModel?.workflow).toBe("combat_sustain");
      expect(sustainModel?.selectionMode).toEqual({ mode: "sustain", hitsRemaining: 2 });

      const casualtyChoice: PendingChoiceDto = {
        ...baseChoice,
        prompt: "assign hits",
        context: partialContext({
          subtype: "assign_casualty",
          outstanding: [{ amount: 3 }],
        }),
      };
      const casualtyModel = deriveChoiceRendererModel(casualtyChoice, "seat_1");
      expect(casualtyModel?.workflow).toBe("combat_casualty");
      expect(casualtyModel?.selectionMode).toEqual({ mode: "casualty", hitsToAssign: 3 });

      const retreatChoice: PendingChoiceDto = {
        ...baseChoice,
        prompt: "announce retreat",
        context: partialContext({ subtype: "announce_retreat" }),
      };
      expect(deriveChoiceRendererModel(retreatChoice, "seat_1")?.workflow).toBe("combat_retreat");
    });

    it("classifies bilateral transactions", () => {
      const proposeChoice: PendingChoiceDto = {
        ...baseChoice,
        prompt: "transaction with Hacan",
        context: partialContext({
          subtype: "propose_transaction",
          target: { Player: "seat_2" },
        }),
        options: [
          { id: "cc3", label: "Swap 3 commodities", kind: "offer", payload: {} },
          { id: "decline", label: "Decline", kind: "decline", payload: {} },
        ],
      };
      const proposeModel = deriveChoiceRendererModel(proposeChoice, "seat_1");
      expect(proposeModel?.workflow).toBe("transaction_propose");
      expect(proposeModel?.selectionMode).toEqual({ mode: "transaction", partnerSeat: "seat_2" });

      const answerChoice: PendingChoiceDto = {
        ...baseChoice,
        prompt: "Accept offer?",
        context: partialContext({
          subtype: "answer_transaction",
          target: { Player: "seat_2" },
        }),
      };
      expect(deriveChoiceRendererModel(answerChoice, "seat_1")?.workflow).toBe(
        "transaction_answer",
      );
    });

    it("classifies agenda voting", () => {
      const voteOutcomeChoice: PendingChoiceDto = {
        ...baseChoice,
        prompt: "vote for which outcome",
        context: partialContext({ subtype: "cast_vote" }),
      };
      expect(deriveChoiceRendererModel(voteOutcomeChoice, "seat_1")?.workflow).toBe(
        "agenda_vote_outcome",
      );

      const votePlanetChoice: PendingChoiceDto = {
        ...baseChoice,
        prompt: "exhaust planet to vote",
        context: partialContext({ subtype: "vote_exhaust_planet" }),
      };
      expect(deriveChoiceRendererModel(votePlanetChoice, "seat_1")?.workflow).toBe(
        "agenda_vote_planets",
      );
    });

    it("classifies reaction windows", () => {
      const reactionChoice: PendingChoiceDto = {
        ...baseChoice,
        prompt: "play Sabotage?",
        context: partialContext({
          subtype: "play_reaction_when_action_card_played",
          optional: true,
        }),
        options: [
          { id: "sabotage", label: "Play Sabotage", kind: "reaction", payload: {} },
          { id: "decline", label: "Pass", kind: "decline", payload: {} },
        ],
      };

      const model = deriveChoiceRendererModel(reactionChoice, "seat_1");
      expect(model?.workflow).toBe("action_card_reaction");
      expect(model?.isOptional).toBe(true);
    });

    it("classifies research_technology workflow", () => {
      const researchChoice: PendingChoiceDto = {
        ...baseChoice,
        prompt: "Research a technology",
        context: partialContext({
          subtype: "research_technology",
        }),
        options: [
          { id: "gd", label: "Gravity Drive", kind: "research", payload: {} },
          { id: "decline", label: "Decline", kind: "decline", payload: {} },
        ],
      };

      const model = deriveChoiceRendererModel(researchChoice, "seat_1");
      expect(model?.workflow).toBe("technology_research");
      expect(model?.isOptional).toBe(true);
    });

    it("keeps an engine-built offer card out of the technology workflow", () => {
      const offer: PendingChoiceDto = {
        ...baseChoice,
        prompt: "Deepwrought commander: reduce this research by 1",
        context: partialContext({ subtype: "deepwrought_reduce_research" }),
        options: [
          { id: "reduce", label: "reduce by 1", kind: "research", payload: {} },
          { id: "decline", label: "Decline", kind: "decline", payload: {} },
        ],
        details: { kind: "offer", card: { title: "Aello" } },
      };
      expect(deriveChoiceRendererModel(offer, "seat_1")?.workflow).toBe("generic_selection");
    });
  });

  describe("typed payload accessors", () => {
    it("getPaymentPayload parses worth, owed, kind, source, and planet name", () => {
      const payload = getPaymentPayload({
        id: "exhaust|jord",
        label: "Jord",
        payload: { worth: 4, owed: 6, kind: "influence", source: "planet", planet_name: "Jord" },
        kind: "",
      });
      expect(payload).toEqual({
        worth: 4,
        owed: 6,
        kind: "influence",
        source: "planet",
        planetName: "Jord",
      });
    });

    it("getMovementPayload parses movement attributes", () => {
      const payload = getMovementPayload({
        id: "move|24|0",
        label: "Cruiser",
        payload: { origin: "24", unit: "cruiser", damaged: true, capacity: 1, gravity_drive: true },
        kind: "",
      });
      expect(payload).toEqual({
        origin: "24",
        unit: "cruiser",
        damaged: true,
        capacity: 1,
        gravity_drive: true,
      });
    });

    it("getTradePayload parses net deltas", () => {
      const payload = getTradePayload({
        id: "cc3",
        label: "Swap",
        payload: { net: 1, their_net: -1 },
        kind: "",
      });
      expect(payload).toEqual({
        net: 1,
        their_net: -1,
      });
    });

    it("getCombatPayload parses combat unit attributes", () => {
      const payload = getCombatPayload({
        id: "sustain|0",
        label: "Dreadnought",
        payload: { unit: "dreadnought", damaged: false },
        kind: "",
      });
      expect(payload).toEqual({
        unit: "dreadnought",
        damaged: false,
      });
    });
  });

  describe("reaction routing", () => {
    const options = (count: number): PendingChoiceDto["options"] => [
      ...Array.from({ length: count }, (_, index) => ({
        id: `reaction:f:X:${index}`,
        label: `Play Card ${index}`,
        kind: "ability",
        payload: {},
      })),
      { id: "decline", label: "Pass", kind: "decline", payload: {} },
    ];
    const reaction = (subtype: string, count: number, source?: Record<string, unknown>) =>
      deriveChoiceRendererModel(
        {
          ...baseChoice,
          options: options(count),
          context: partialContext({ subtype, optional: true, ...(source ? { source } : {}) }),
        },
        "seat_1",
      )?.workflow;

    it("routes the outer window, however many options it has", () => {
      expect(reaction("reaction_when_ACTION_CARD_PLAYED", 1, { Reaction: "X" })).toBe(
        "action_card_reaction",
      );
      expect(reaction("reaction_after_SYSTEM_ACTIVATED", 6, { Reaction: "X" })).toBe(
        "action_card_reaction",
      );
    });

    it("routes the inner card pick and the two reaction abilities", () => {
      expect(reaction("play_reaction_after_SYSTEM_ACTIVATED", 2)).toBe("action_card_reaction");
      expect(reaction("instinct_training_cancel", 1, { Content: "it" })).toBe(
        "action_card_reaction",
      );
      expect(reaction("l1z1x_agent_swap", 1, { Content: "l1z1xagent" })).toBe(
        "action_card_reaction",
      );
    });
  });
});
