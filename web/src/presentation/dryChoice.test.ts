import { describe, expect, it } from "vitest";
import { detectStrategicAction } from "./strategicAction.ts";
import { buildDryChoice, nextDryStep, isDryNonce } from "./dryChoice.ts";
import { describeStrategySecondary } from "./strategySecondary.ts";
import { describeCommandTokens, maxPurchases } from "./commandTokens.ts";
import { isPlanetSelectionChoice } from "./choiceModel.ts";
import { resolveMapTargetSelection } from "./planetSelection.ts";
import { resolveStep } from "./secondaryPlan.ts";
import { boardWith, playedLog, player } from "../test/secondaryPrepFixtures.ts";

const actionFor = (card: string, name: string) => {
  const players = [player("a", { strategy_cards: [card] }), player("b")];
  return {
    players,
    action: detectStrategicAction({ events: playedLog(name), players, phase: "action" })!,
  };
};

describe("dry choices: the engine's questions, synthesized from the viewer's own data", () => {
  it("follow/skip for every card has the engine's yes/no shape and prompt", () => {
    const cards: [string, string, RegExp][] = [
      ["pok7technology", "Technology", /^spend a strategy token and 4 resources to research$/],
      ["pok2diplomacy", "Diplomacy", /ready two planets$/],
      ["pok3politics", "Politics", /draw two action cards$/],
      ["pok4construction", "Construction", /build a structure$/],
      ["pok5trade", "Trade", /replenish commodities$/],
      ["pok6warfare", "Warfare", /produce at home$/],
      ["pok8imperial", "Imperial", /secret objective$/],
    ];
    for (const [card, name, prompt] of cards) {
      const { action, players } = actionFor(card, name);
      const dry = buildDryChoice({ action, viewer: players[1], board: undefined, step: "secondary" })!;
      expect(dry.choice.prompt).toMatch(prompt);
      expect(dry.choice.options.map((o) => o.id)).toEqual(["no", "yes"]);
      expect(dry.choice.context?.subtype).toBe("strategy_secondary");
      expect(dry.choice.actor).toBe("b");
      expect(isDryNonce(dry.choice.nonce)).toBe(true);
      // The real panel's own parser accepts it: it is what the engine would send.
      const view = describeStrategySecondary(dry.choice)!;
      expect(view.cardName).toBe(name);
      expect(view.tokensLeft).toBe(2);
      expect(view.costsToken).toBe(true);
    }
  });

  it("Trade carries the replenish warning as its approximation", () => {
    const { action, players } = actionFor("pok5trade", "Trade");
    const dry = buildDryChoice({ action, viewer: players[1], board: undefined, step: "secondary" })!;
    expect(dry.approximate).toMatch(/replenishes you/);
    const plain = actionFor("pok3politics", "Politics");
    expect(buildDryChoice({ action: plain.action, viewer: plain.players[1], board: undefined, step: "secondary" })!.approximate).toBeNull();
  });

  it("Technology lists researchable technologies as research options, flagged approximate", () => {
    const { action, players } = actionFor("pok7technology", "Technology");
    const dry = buildDryChoice({ action, viewer: players[1], board: undefined, step: "tech" })!;
    expect(dry.choice.context?.subtype).toBe("research_technology");
    expect(dry.choice.options.length).toBeGreaterThan(0);
    expect(dry.choice.options.at(-1)?.id).toBe("decline");
    expect(dry.choice.options.slice(0, -1).every((o) => o.kind === "research")).toBe(true);
    expect(dry.choice.context?.source).toEqual({ StrategyCard: { card: "Technology", secondary: true } });
    expect(dry.approximate).toMatch(/real question opens/);
    // A prepared tech resolves against the dry list the same way as against the real one.
    const id = dry.choice.options[0].id;
    expect(resolveStep({ card: "pok7technology", follow: true, tech: id }, dry.choice, "b")).toMatchObject({
      kind: "option",
      optionId: id,
    });
  });

  it("Diplomacy: exhausted own planets with planet payloads, the map-selection shape", () => {
    const { action, players } = actionFor("pok2diplomacy", "Diplomacy");
    const board = boardWith([
      { system: "18", planet: "jord", owner: "b", exhausted: true },
      { system: "26", planet: "lodor", owner: "b", exhausted: true },
      { system: "27", planet: "arnor", owner: "b", exhausted: false },
      { system: "28", planet: "mecatol", owner: "a", exhausted: true },
    ]);
    const first = buildDryChoice({ action, viewer: players[1], board, step: "planet1" })!;
    expect(first.choice.options.map((o) => o.id)).toEqual(["jord", "lodor"]);
    expect(first.choice.options[0]).toMatchObject({ kind: "ready", payload: { planet: "jord", system: "18" } });
    expect(first.choice.context?.subtype).toBe("ready_planet");
    expect(isPlanetSelectionChoice(first.choice)).toBe(true);
    // Clicking the planet on the map selects its option exactly as in the real flow.
    expect(resolveMapTargetSelection(first.choice, "18", "jord", board)).toMatchObject({
      kind: "select",
      optionId: "jord",
      planetId: "jord",
    });
    const second = buildDryChoice({ action, viewer: players[1], board, step: "planet2", taken: ["jord"] })!;
    expect(second.choice.options.map((o) => o.id)).toEqual(["lodor"]);
    // No exhausted planet, no question.
    expect(buildDryChoice({ action, viewer: players[1], board: boardWith([]), step: "planet1" })).toBeNull();
  });

  it("Construction: PDS and space dock sites on own planets, minus structures already there", () => {
    const { action, players } = actionFor("pok4construction", "Construction");
    const board = boardWith([{ system: "18", planet: "jord", owner: "b" }]);
    board.systems["18"].units.push({ unit_type: "pds", owner: "b", planet: "jord", damaged: false });
    const dry = buildDryChoice({ action, viewer: players[1], board, step: "site" })!;
    expect(dry.choice.options.map((o) => o.id)).toEqual(["spacedock|18|jord", "decline"]);
    expect(dry.choice.options[0].payload).toMatchObject({ planet: "jord", system: "18", unit: "spacedock" });
    expect(dry.approximate).toMatch(/real question may offer fewer/);
    expect(
      resolveStep(
        { card: "pok4construction", follow: true, structure: { unit: "spacedock", planet: "jord" } },
        dry.choice,
        "b",
      ),
    ).toMatchObject({ kind: "option", optionId: "spacedock|18|jord" });
  });

  it("Leadership: the purchase window with pools and influence from planets and trade goods", () => {
    const { action, players } = actionFor("pok1leadership", "Leadership");
    const viewer = { ...players[1], trade_goods: 1 };
    const board = boardWith([
      { system: "18", planet: "jord", owner: "b" },
      { system: "26", planet: "lodor", owner: "b", exhausted: true },
    ]);
    const dry = buildDryChoice({ action, viewer, board, step: "secondary" })!;
    expect(dry.choice.prompt).toBe("spend 3 influence for a command token");
    expect(dry.choice.details?.costs_token).toBe(false);
    const view = describeCommandTokens(dry.choice, true)!;
    expect(view.mode).toBe("gain");
    expect(view.current).toEqual({ tactic: 3, fleet: 3, strategic: 2 });
    // jord (2 influence, ready) + 1 trade good; exhausted lodor does not count.
    expect(view.purchase?.influence).toBe(3);
    expect(maxPurchases(view)).toBe(1);
    expect(dry.approximate).toMatch(/payment is planned then/);
  });

  it("steps follow the card: tech, planets (two), site, nothing for the rest", () => {
    expect(nextDryStep("technology", "secondary", { planets: 0 })).toBe("tech");
    expect(nextDryStep("diplomacy", "secondary", { planets: 0 })).toBe("planet1");
    expect(nextDryStep("diplomacy", "planet1", { planets: 1 })).toBe("planet2");
    expect(nextDryStep("diplomacy", "planet2", { planets: 2 })).toBeNull();
    expect(nextDryStep("construction", "secondary", { planets: 0 })).toBe("site");
    expect(nextDryStep("politics", "secondary", { planets: 0 })).toBeNull();
    expect(nextDryStep("leadership", "secondary", { planets: 0 })).toBeNull();
  });
});
