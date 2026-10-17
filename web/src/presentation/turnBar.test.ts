import { describe, expect, it } from "vitest";
import type { PlayerView } from "../protocol/types.ts";
import {
  deriveReadOnlyTurnBar,
  deriveTurnBar,
  isTurnMenuChoice,
  type TurnBarModel,
} from "./turnBar.ts";

const opt = (id: string, label: string, kind = "action") => ({ id, label, kind, payload: {} });

const tokens = { tactic: 3, fleet: 3, strategy: 2 };
const partner = (seat: string, faction: string, extra: Record<string, unknown> = {}) => ({
  seat,
  faction,
  available: true,
  in_contact: true,
  reason: null,
  trade_goods: 2,
  commodities: 3,
  promissory_notes: 1,
  ...extra,
});

function menu(
  options: ReturnType<typeof opt>[],
  details: Record<string, unknown> = {},
  prompt = "action phase",
) {
  return { prompt, options, details: { kind: "turn_menu", closing: false, tokens, ...details } };
}

const twoCards = [
  { card: "pok2diplomacy", used: false, option: "strategic|pok2diplomacy" },
  { card: "pok8imperial", used: false, option: "strategic|pok8imperial" },
];

function bar(choice: Parameters<typeof deriveTurnBar>[0], player?: PlayerView): TurnBarModel {
  const model = deriveTurnBar(choice, player);
  if (!model) throw new Error("expected a bar model");
  return model;
}

describe("deriveTurnBar: what it maps", () => {
  it("recognises the action-phase menu and the end-turn question, nothing else", () => {
    expect(isTurnMenuChoice({ prompt: "action phase" })).toBe(true);
    expect(isTurnMenuChoice({ prompt: "x", context: partialContext({ subtype: "end_turn" }) })).toBe(true);
    expect(isTurnMenuChoice({ prompt: "pick a planet" })).toBe(false);
    expect(deriveTurnBar({ prompt: "pick a planet", options: [opt("a", "a")] })).toBeNull();
  });

  it("gives one Strategic button per held card, each with its name, effect and state", () => {
    const model = bar(
      menu(
        [
          opt("strategic|pok2diplomacy", "take the strategic action of 2. Diplomacy"),
          opt("strategic|pok8imperial", "take the strategic action of 8. Imperial"),
          opt("tactical", "take a tactical action"),
        ],
        { strategy_cards: twoCards },
      ),
    );
    expect(model.strategic.map((b) => b.label)).toEqual(["2. Diplomacy", "8. Imperial"]);
    expect(model.strategic.map((b) => b.optionId)).toEqual([
      "strategic|pok2diplomacy",
      "strategic|pok8imperial",
    ]);
    expect(model.strategic.every((b) => b.enabled && b.effect.length > 0)).toBe(true);
    expect(model.strategic[0].effect.length).toBeLessThanOrEqual(96);
    expect(model.strategic[0].info.text).toMatch(/system/i);
    expect(model.strategic[0].info.secondary).toMatch(/^Secondary:/);
  });

  it("keeps a single card as one Strategic action button", () => {
    const model = bar(
      menu([opt("strategic", "take your strategic action"), opt("tactical", "t")], {
        strategy_cards: [{ card: "pok7technology", used: false, option: "strategic" }],
      }),
    );
    expect(model.strategic).toHaveLength(1);
    expect(model.strategic[0].label).toBe("Strategic action");
    expect(model.strategic[0].cardName).toBe("7. Technology");
    expect(model.strategic[0].optionId).toBe("strategic");
  });

  it("disables a used card with its reason and sends the survivor's bare id", () => {
    const model = bar(
      menu([opt("strategic", "take your strategic action"), opt("tactical", "t")], {
        strategy_cards: [
          { card: "pok2diplomacy", used: true, option: null },
          { card: "pok8imperial", used: false, option: "strategic" },
        ],
      }),
    );
    const [used, ready] = model.strategic;
    expect(used).toMatchObject({
      enabled: false,
      used: true,
      reason: "Already used this round",
      optionId: null,
    });
    expect(used.info.chips[0].label).toBe("Used this round");
    expect(ready).toMatchObject({ enabled: true, optionId: "strategic" });
  });

  it("rebuilds the cards from the seat snapshot when the server sends no details", () => {
    const player = {
      strategy_cards: ["pok2diplomacy", "pok8imperial"],
      exhausted_strategy_cards: ["pok2diplomacy"],
      tactic_tokens: 2,
      fleet_tokens: 3,
      strategic_tokens: 1,
      action_cards_count: 0,
      passed: false,
    } as unknown as PlayerView;
    const model = bar(
      { prompt: "action phase", options: [opt("strategic", "s"), opt("tactical", "t")] },
      player,
    );
    expect(model.strategic.map((b) => [b.cardId, b.enabled, b.optionId])).toEqual([
      ["pok2diplomacy", false, null],
      ["pok8imperial", true, "strategic"],
    ]);
    expect(model.tokens).toEqual({ tactic: 2, fleet: 3, strategy: 1 });
  });

  it("refuses to hide a strategic option it cannot place", () => {
    expect(
      deriveTurnBar(
        menu([opt("strategic|zzz", "s"), opt("pass", "pass")], { strategy_cards: twoCards }),
      ),
    ).toBeNull();
  });

  it("collapses every transaction row into one Trade button with a partner picker", () => {
    const model = bar(
      menu(
        [
          opt("tactical", "t"),
          opt("component|trade|hacan", "open a transaction with hacan", "open_transaction"),
          opt("component|trade|letnev", "open a transaction with letnev", "open_transaction"),
        ],
        {
          partners: [
            partner("b", "hacan"),
            partner("c", "letnev", { trade_goods: 0 }),
            partner("d", "xxcha", {
              available: false,
              in_contact: false,
              reason: "No contact: not neighbours",
            }),
          ],
        },
      ),
    );
    expect(model.trade.enabled).toBe(true);
    expect(model.trade.available).toBe(2);
    expect(model.trade.partners.map((p) => [p.faction, p.optionId, p.available])).toEqual([
      ["hacan", "component|trade|hacan", true],
      ["letnev", "component|trade|letnev", true],
      ["xxcha", null, false],
    ]);
    expect(model.trade.partners[2].reason).toBe("No contact: not neighbours");
    expect(model.trade.partners[0]).toMatchObject({ tradeGoods: 2, commodities: 3 });
    // The partners are not also listed as components.
    expect(model.components.count).toBe(0);
  });

  it("names the partners from the options alone when there are no details", () => {
    const model = bar({
      prompt: "action phase",
      options: [
        opt("pass", "pass"),
        opt("component|trade|hacan", "open a transaction with hacan", "open_transaction"),
      ],
    });
    expect(model.trade.partners).toHaveLength(1);
    expect(model.trade.partners[0]).toMatchObject({
      faction: "hacan",
      optionId: "component|trade|hacan",
    });
  });

  it("disables Trade with the shared reason when nobody can be opened", () => {
    const model = bar(
      menu([opt("pass", "pass")], {
        partners: [
          partner("b", "hacan", { available: false, reason: "Already traded with them this turn" }),
        ],
      }),
    );
    expect(model.trade).toMatchObject({
      enabled: false,
      reason: "Already traded with them this turn",
    });
  });

  it("groups component actions and keeps action cards apart with a count", () => {
    const model = bar(
      menu([
        opt("pass", "pass"),
        opt("faction|orbital_drop", "Orbital Drop: spend a strategy token to land 2 infantry", "component"),
        opt("component|tech|sr", "use Sling Relay", "component"),
        opt("component|leader|carth", "Carth of Golden Sands", "component"),
        opt("action_card|0", "play Reactor Meltdown", "component"),
        opt("action_card|2", "play Ghost Ship", "component"),
        opt("mystery|1", "something new", "component"),
      ]),
    );
    expect(model.components.groups.map((g) => g.title)).toEqual([
      "Faction abilities",
      "Technologies",
      "Leaders",
      "Other",
    ]);
    expect(model.components.count).toBe(4);
    const drop = model.components.groups[0].items[0];
    expect(drop.optionId).toBe("faction|orbital_drop");
    expect(drop.info.chips[0].label).toBe("Cost: 1 strategy token");
    expect(model.components.groups[1].items[0].label).toBe("Sling Relay");
    expect(model.actionCards.count).toBe(2);
    expect(model.actionCards.items.map((i) => i.label)).toEqual(["Reactor Meltdown", "Ghost Ship"]);
    expect(model.actionCards.items[0].info.text).toMatch(/space dock/i);
    expect(model.actionCards.items[0].optionId).toBe("action_card|0");
  });

  it("says why Components and Action cards are empty", () => {
    const model = bar(menu([opt("pass", "pass")]), {
      action_cards_count: 3,
      tactic_tokens: 1,
      fleet_tokens: 1,
      strategic_tokens: 1,
    } as unknown as PlayerView);
    expect(model.components).toMatchObject({ enabled: false, reason: "Nothing usable now" });
    expect(model.actionCards).toMatchObject({
      enabled: false,
      reason: "3 in hand, none playable now",
    });
  });
});

describe("deriveTurnBar: the fixed buttons", () => {
  it("offers Tactical with the tokens left and submits its option", () => {
    const model = bar(menu([opt("tactical", "take a tactical action"), opt("strategic", "s")]));
    expect(model.tactical).toMatchObject({ enabled: true, optionId: "tactical" });
    expect(model.tactical.hint).toBe("3 tactic tokens left");
  });

  it("explains a missing Tactical: no tokens, or already acted", () => {
    const empty = bar(
      menu([opt("pass", "pass")], { tokens: { tactic: 0, fleet: 1, strategy: 1 } }),
    );
    expect(empty.tactical).toMatchObject({ enabled: false, reason: "No tactic tokens" });
    const acted = bar(menu([opt("end_turn", "end your turn", "end_turn")], { closing: true }));
    expect(acted.tactical).toMatchObject({ enabled: false, reason: "Already acted" });
  });

  it("keeps Pass disabled until the cards are used, and says how many", () => {
    const two = bar(
      menu([opt("strategic|pok8imperial", "s"), opt("tactical", "t")], {
        strategy_cards: [
          { card: "pok2diplomacy", used: true, option: null },
          { card: "pok8imperial", used: false, option: "strategic|pok8imperial" },
        ],
      }),
    );
    expect(two.pass).toMatchObject({ enabled: false, reason: "Use your strategy card first" });
    const both = bar(
      menu([opt("strategic|pok2diplomacy", "s"), opt("strategic|pok8imperial", "s")], {
        strategy_cards: twoCards,
      }),
    );
    expect(both.pass.reason).toBe("Use both cards first");
    const ready = bar(menu([opt("pass", "pass"), opt("tactical", "t")]));
    expect(ready.pass).toMatchObject({ enabled: true, optionId: "pass", reason: null });
  });

  it("closes the turn with End turn enabled and Pass disabled as already acted", () => {
    const model = bar(
      menu(
        [
          opt("end_turn", "end your turn", "end_turn"),
          opt("component|trade|hacan", "open a transaction with hacan", "open_transaction"),
        ],
        { closing: true, partners: [partner("b", "hacan")] },
        "end your turn",
      ),
    );
    expect(model.mode).toBe("closing");
    expect(model.end).toMatchObject({ enabled: true, optionId: "end_turn" });
    expect(model.pass).toMatchObject({ enabled: false, reason: "Already acted: end your turn" });
    expect(model.trade.enabled).toBe(true);
  });

  it("keeps End turn disabled in the action phase", () => {
    const model = bar(menu([opt("pass", "pass")]));
    expect(model.mode).toBe("active");
    expect(model.end).toMatchObject({ enabled: false, reason: "Act or pass first" });
  });

  it("keeps the bar's end id when the harness uses a different option id", () => {
    const model = bar(menu([opt("end", "end your turn", "end_turn")], { closing: true }));
    expect(model.end.optionId).toBe("end");
  });

  it("refuses a menu with two Pass options", () => {
    expect(deriveTurnBar(menu([opt("pass", "pass"), opt("pass", "pass")]))).toBeNull();
  });
});

describe("deriveReadOnlyTurnBar", () => {
  const player = {
    id: "b",
    passed: false,
    strategy_cards: ["pok2diplomacy", "pok8imperial"],
    exhausted_strategy_cards: ["pok2diplomacy"],
    tactic_tokens: 3,
    fleet_tokens: 2,
    strategic_tokens: 1,
    action_cards_count: 2,
  } as unknown as PlayerView;

  it("disables everything with the reason and names whose turn it is", () => {
    const model = deriveReadOnlyTurnBar(player, "a");
    expect(model.mode).toBe("readonly");
    expect(model.waitingFor).toBe("a");
    expect(model.tactical).toMatchObject({ enabled: false, reason: "Not your turn" });
    expect(model.pass.reason).toBe("Not your turn");
    expect(model.end.reason).toBe("Not your turn");
    expect(model.trade).toMatchObject({ enabled: false, reason: "Not your turn" });
    expect(model.tokens).toEqual({ tactic: 3, fleet: 2, strategy: 1 });
    expect(model.strategic.map((b) => [b.label, b.reason, b.used])).toEqual([
      ["2. Diplomacy", "Already used this round", true],
      ["8. Imperial", "Not your turn", false],
    ]);
    expect(model.strategic.every((b) => b.optionId === null)).toBe(true);
  });

  it("says so when the seat has passed", () => {
    const model = deriveReadOnlyTurnBar({ ...player, passed: true }, "a");
    expect(model.heading).toBe("You have passed");
    expect(model.waitingFor).toBeNull();
    expect(model.tactical.reason).toBe("You have passed");
  });
});

import { planetsSummary as econSummary } from "./turnBar.ts";
import { partialContext } from "../test/wire.ts";
describe("planetsSummary", () => {
  it("counts the exhausted cultural planets", () => {
    expect(
      econSummary([
        { id: "a", name: "A", trait: "cultural", exhausted: true },
        { id: "b", name: "B", trait: "cultural", exhausted: false },
      ]),
    ).toBe("Cultural: A, B (ready); readies 1 of 2");
    expect(econSummary([])).toContain("nothing to ready");
  });
});
