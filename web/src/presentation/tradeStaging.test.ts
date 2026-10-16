import { describe, it, expect } from "vitest";
import type { ChoiceOption } from "../protocol/types.ts";
import {
  emptyStaged,
  freeFormId,
  proposalId,
  nearestDeals,
  parseOfferPrompt,
  stageableDeals,
  stagedKey,
  stagedToDealId,
  stagingLimits,
  whyNoDeal,
  type Staged,
} from "./tradeStaging.ts";

const offer = (id: string, payload: Record<string, unknown> = {}): ChoiceOption => ({
  id,
  label: id,
  kind: "offer",
  payload,
});

/** A catalog shaped like the engine's `offer_options` for a Hacan-style proposer. */
const CATALOG: ChoiceOption[] = [
  offer("ss"),
  offer("pnra:jolnar:3", { note: "ra:jolnar" }),
  offer("pnceasefire:jolnar:0", { note: "ceasefire:jolnar", gift: true }),
  offer("acsabo1:1"),
  offer("sodestroy:1"),
  offer("frcultural:1"),
  offer("cc3"),
  offer("cc1"),
  offer("ct3:2"),
  offer("tc2:3"),
  offer("c4:0"),
  offer("0:1"),
  offer("1:0"),
  offer("1:1"),
  offer("2:3"),
  offer("npcf:jolnar:2", { received_promissory: "cf:jolnar" }),
  offer("cpcf:jolnar:2", { received_promissory: "cf:jolnar" }),
  offer("pcra:jolnar:3", { promissory: "ra:jolnar" }),
  offer("nnra:jolnar>cf:jolnar", { promissory: "ra:jolnar", received_promissory: "cf:jolnar" }),
  offer("cnsabo1>cf:jolnar", { action_card: "sabo1", received_promissory: "cf:jolnar" }),
  { id: "decline", label: "Offer nothing", kind: "decline", payload: {} },
];

describe("staged <-> deal id", () => {
  const deals = stageableDeals(CATALOG);

  it("leaves decline out of the stageable deals", () => {
    expect(deals.map((d) => d.id)).not.toContain("decline");
    expect(deals).toHaveLength(CATALOG.length - 1);
  });

  it("round-trips every listed deal to its own id", () => {
    for (const d of deals) {
      expect(stagedToDealId(d.staged, deals), d.id).toBe(d.id);
    }
  });

  it("gives every listed deal its own staged combination", () => {
    const keys = deals.map((d) => stagedKey(d.staged));
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("reads the amounts and items each shape stands for", () => {
    const by = (id: string) => deals.find((d) => d.id === id)!.staged;
    expect(by("cc3")).toMatchObject({ give: { commodities: 3 }, receive: { commodities: 3 } });
    expect(by("ct3:2")).toMatchObject({ give: { commodities: 3 }, receive: { tradeGoods: 2 } });
    expect(by("tc2:3")).toMatchObject({ give: { tradeGoods: 2 }, receive: { commodities: 3 } });
    expect(by("c4:0")).toMatchObject({ give: { commodities: 4 }, receive: { tradeGoods: 0 } });
    expect(by("2:3")).toMatchObject({ give: { tradeGoods: 2 }, receive: { tradeGoods: 3 } });
    // A note sale is paid by the partner; the price is on the receive side.
    expect(by("pnra:jolnar:3")).toMatchObject({
      give: { note: "ra:jolnar" },
      receive: { tradeGoods: 3 },
    });
    expect(by("pnceasefire:jolnar:0").receive.tradeGoods).toBe(0);
    // Asking for the partner's note: the price is on the give side, the note on the receive side.
    expect(by("npcf:jolnar:2")).toMatchObject({
      give: { tradeGoods: 2, note: null },
      receive: { note: "cf:jolnar", tradeGoods: 0 },
    });
    expect(by("cpcf:jolnar:2")).toMatchObject({ give: { commodities: 2 } });
    expect(by("pcra:jolnar:3")).toMatchObject({
      give: { note: "ra:jolnar" },
      receive: { commodities: 3 },
    });
    expect(by("nnra:jolnar>cf:jolnar")).toMatchObject({
      give: { note: "ra:jolnar" },
      receive: { note: "cf:jolnar" },
    });
    expect(by("cnsabo1>cf:jolnar")).toMatchObject({
      give: { actionCard: "sabo1" },
      receive: { note: "cf:jolnar" },
    });
    expect(by("acsabo1:1")).toMatchObject({ give: { actionCard: "sabo1" }, receive: { tradeGoods: 1 } });
    expect(by("sodestroy:1").give.secret).toBe("destroy");
    expect(by("frcultural:1").give.fragments).toEqual(["cultural"]);
    expect(by("ss")).toMatchObject({ give: { support: true }, receive: { support: true } });
  });

  it("finds nothing for combinations the server does not list", () => {
    const s: Staged = emptyStaged();
    s.give.tradeGoods = 3;
    s.receive.tradeGoods = 3;
    expect(stagedToDealId(s, deals)).toBeNull();
    // a bundle of two items is not any listed shape
    const bundle = emptyStaged();
    bundle.give.note = "ra:jolnar";
    bundle.give.tradeGoods = 1;
    bundle.receive.tradeGoods = 3;
    expect(stagedToDealId(bundle, deals)).toBeNull();
    expect(stagedToDealId(emptyStaged(), deals)).toBeNull();
  });

  it("distinguishes amounts: 2 for 3 is listed, 3 for 2 is not", () => {
    const listed = emptyStaged();
    listed.give.tradeGoods = 2;
    listed.receive.tradeGoods = 3;
    expect(stagedToDealId(listed, deals)).toBe("2:3");
    const flipped = emptyStaged();
    flipped.give.tradeGoods = 3;
    flipped.receive.tradeGoods = 2;
    expect(stagedToDealId(flipped, deals)).toBeNull();
  });

  it("suggests the nearest listed deals, never the exact match", () => {
    const s = emptyStaged();
    s.give.tradeGoods = 3;
    s.receive.tradeGoods = 3;
    const near = nearestDeals(s, deals, 3);
    expect(near).toHaveLength(3);
    expect(near[0].id).toBe("2:3");
    const exact = emptyStaged();
    exact.give.tradeGoods = 2;
    exact.receive.tradeGoods = 3;
    expect(nearestDeals(exact, deals).map((d) => d.id)).not.toContain("2:3");
  });

  it("reports the limits of the list and says why a combination fails", () => {
    const limits = stagingLimits(deals);
    expect(limits.give.maxTradeGoods).toBe(2);
    expect(limits.receive.maxTradeGoods).toBe(3);
    expect(limits.give.notes).toContain("ra:jolnar");
    expect(limits.receive.notes).toEqual(["cf:jolnar"]);
    expect(limits.receive.actionCards).toEqual([]);
    expect(limits.give.support).toBe(true);
    const s = emptyStaged();
    s.give.tradeGoods = 3;
    expect(whyNoDeal(s, deals).join(" ")).toContain("at most 2 trade goods");
    expect(whyNoDeal(emptyStaged(), deals)).toEqual(["Nothing is staged yet."]);
  });
});

describe("parseOfferPrompt", () => {
  it("reads the answerer's side of a prompt", () => {
    const parsed = parseOfferPrompt(
      "Hacan gives 2 trade goods, the action card sabo1 for 3 commodities, ra:jolnar -- accept?",
    );
    expect(parsed?.proposer).toBe("Hacan");
    // The answerer gives what the proposer asked for and receives what the proposer gives.
    expect(parsed?.staged.give).toMatchObject({ commodities: 3, note: "ra:jolnar" });
    expect(parsed?.staged.receive).toMatchObject({ tradeGoods: 2, actionCard: "sabo1" });
  });

  it("reads nothing, fragments and secrets", () => {
    const parsed = parseOfferPrompt(
      "Sol gives 2 relic fragments, the secret objective faa for nothing -- accept?",
    );
    expect(parsed?.staged.receive.fragments).toHaveLength(2);
    expect(parsed?.staged.receive.secret).toBe("faa");
    expect(parsed?.staged.give.commodities).toBe(0);
  });

  it("returns null for any other prompt", () => {
    expect(parseOfferPrompt("something else entirely")).toBeNull();
  });
});

describe("free-form proposals", () => {
  const deals = stageableDeals(CATALOG);
  const staged = (give: Partial<Staged["give"]>, receive: Partial<Staged["receive"]>): Staged => {
    const s = emptyStaged();
    return { give: { ...s.give, ...give }, receive: { ...s.receive, ...receive } };
  };

  it("writes the id the engine's free_form_id reads", () => {
    expect(freeFormId(staged({ tradeGoods: 7, commodities: 1, note: "ra:jolnar" }, { commodities: 3 }))).toBe(
      "x;gt=7;gc=1;gn=ra:jolnar;rc=3;",
    );
    expect(freeFormId(staged({ fragments: ["cultural", "hazardous"] }, { tradeGoods: 1 }))).toBe(
      "x;gf=cultural,hazardous;rt=1;",
    );
  });

  it("has no free-form id for nothing or for the Support swap", () => {
    expect(freeFormId(emptyStaged())).toBeNull();
    expect(freeFormId(staged({ support: true }, { support: true }))).toBeNull();
  });

  it("prefers the listed id, falls back to free-form only when allowed and affordable", () => {
    const listed = staged({ tradeGoods: 1 }, { tradeGoods: 1 });
    expect(proposalId(listed, deals, true)).toBe("1:1");
    const odd = staged({ tradeGoods: 6 }, { commodities: 2 });
    expect(proposalId(odd, deals, false)).toBeNull();
    expect(proposalId(odd, deals, true)).toBe("x;gt=6;rc=2;");
    expect(proposalId(odd, deals, true, { tradeGoods: 5, commodities: 0 }, null)).toBeNull();
    expect(proposalId(odd, deals, true, { tradeGoods: 6, commodities: 0 }, { tradeGoods: 0, commodities: 1 })).toBeNull();
    expect(whyNoDeal(odd, deals, true, { tradeGoods: 5, commodities: 0 }, null)).toEqual(["You hold only 5 trade goods."]);
  });
});
