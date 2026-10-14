import { describe, it, expect } from "vitest";
import { decodeTradeOption } from "./tradeDecoder.ts";
import { ChoiceOptionDto } from "../protocol/types.ts";

describe("tradeDecoder", () => {
  it("decodes mutual support for the throne (ss)", () => {
    const opt: ChoiceOptionDto = {
      id: "ss",
      kind: "offer",
      label: "Swap Support for the Throne",
      payload: { net: 1, their_net: 1 },
    };
    const decoded = decodeTradeOption(opt);
    expect(decoded.category).toBe("mutual_support");
    expect(decoded.net).toBe(1);
    expect(decoded.their_net).toBe(1);
  });

  it("decodes commodity swap (cc3)", () => {
    const opt: ChoiceOptionDto = {
      id: "cc3",
      kind: "offer",
      label: "swap 3 commodities each",
      payload: { net: 3, their_net: 3 },
    };
    const decoded = decodeTradeOption(opt);
    expect(decoded.category).toBe("commodity_swap");
    expect(decoded.details.giveCommodities).toBe(3);
    expect(decoded.details.receiveCommodities).toBe(3);
  });

  it("decodes commodity gift (c2:0)", () => {
    const opt: ChoiceOptionDto = {
      id: "c2:0",
      kind: "offer",
      label: "gift 2 commodities",
      payload: { net: -2, their_net: 2 },
    };
    const decoded = decodeTradeOption(opt);
    expect(decoded.category).toBe("commodity_swap");
    expect(decoded.details.giveCommodities).toBe(2);
    expect(decoded.details.receiveCommodities).toBeUndefined();
  });

  it("decodes goods exchange (ct2:3, tc1:2, and 1:2)", () => {
    const ctOpt: ChoiceOptionDto = {
      id: "ct2:3",
      kind: "offer",
      label: "give 2 commodities for 3 trade goods",
      payload: { net: 1, their_net: -1 },
    };
    const decodedCt = decodeTradeOption(ctOpt);
    expect(decodedCt.category).toBe("goods_exchange");
    expect(decodedCt.details.giveCommodities).toBe(2);
    expect(decodedCt.details.receiveTradeGoods).toBe(3);

    const tcOpt: ChoiceOptionDto = {
      id: "tc1:2",
      kind: "offer",
      label: "give 1 trade goods for 2 commodities",
    };
    const decodedTc = decodeTradeOption(tcOpt);
    expect(decodedTc.category).toBe("goods_exchange");
    expect(decodedTc.details.giveTradeGoods).toBe(1);
    expect(decodedTc.details.receiveCommodities).toBe(2);

    const tgOpt: ChoiceOptionDto = {
      id: "1:2",
      kind: "offer",
      label: "give 1 for 2",
    };
    const decodedTg = decodeTradeOption(tgOpt);
    expect(decodedTg.category).toBe("goods_exchange");
    expect(decodedTg.details.giveTradeGoods).toBe(1);
    expect(decodedTg.details.receiveTradeGoods).toBe(2);
  });

  it("decodes promissory notes (pnsupport:sol:3)", () => {
    const opt: ChoiceOptionDto = {
      id: "pnsupport:sol:3",
      kind: "offer",
      label: "offer Support for 3 TG",
      payload: { net: 3, their_net: 1 },
    };
    const decoded = decodeTradeOption(opt);
    expect(decoded.category).toBe("promissory");
    expect(decoded.details.promissoryNote).toBe("support:sol");
    expect(decoded.details.price).toBe(3);
  });

  it("decodes action cards and fragments", () => {
    const acOpt: ChoiceOptionDto = {
      id: "acmorale_boost:2",
      kind: "offer",
      label: "offer Morale Boost for 2 TG",
    };
    const decodedAc = decodeTradeOption(acOpt);
    expect(decodedAc.category).toBe("other");
    expect(decodedAc.details.actionCard).toBe("morale_boost");
    expect(decodedAc.details.price).toBe(2);

    const frOpt: ChoiceOptionDto = {
      id: "frcultural:4",
      kind: "offer",
      label: "offer Cultural Fragment for 4 TG",
    };
    const decodedFr = decodeTradeOption(frOpt);
    expect(decodedFr.category).toBe("other");
    expect(decodedFr.details.fragment).toBe("cultural");
    expect(decodedFr.details.price).toBe(4);
  });

  // --- conflict / edge-case paths (finding #15) ---

  it("prefix precedence: ct prefix beats the generic c-with-colon branch", () => {
    // "ct3:2" starts with 'ct', which is case 6 (commodity→TG).
    // It also starts with 'c' and contains ':', which is case 8 (commodity gift).
    // Case 6 must win because it appears first in the decoder.
    const opt: ChoiceOptionDto = {
      id: "ct3:2",
      kind: "offer",
      label: "give 3 commodities for 2 trade goods",
    };
    const decoded = decodeTradeOption(opt);
    expect(decoded.category).toBe("goods_exchange");
    expect(decoded.details.giveCommodities).toBe(3);
    expect(decoded.details.receiveTradeGoods).toBe(2);
    // Must NOT be misread as a commodity gift
    expect(decoded.details.receiveTradeGoods).not.toBeUndefined();
  });

  it("payload fields take priority over prefix heuristics (pn id + swap_commodities payload)", () => {
    // The id starts with 'pn' (promissory branch) but the payload carries
    // swap_commodities — payload.note is absent, so the pn branch is still
    // exercised (id prefix wins over the extra payload field).
    // This verifies that a mismatched payload does not corrupt the category.
    const opt: ChoiceOptionDto = {
      id: "pnsupport:sol:0",
      kind: "offer",
      label: "Support for the Throne (gift)",
      payload: { swap_commodities: 2, gift: true },
    };
    const decoded = decodeTradeOption(opt);
    // pn prefix matches first; swap_commodities in payload is ignored
    expect(decoded.category).toBe("promissory");
    expect(decoded.details.promissoryNote).toBe("support:sol");
    expect(decoded.details.price).toBe(0);
  });

  it('unknown id format falls back to category "other"', () => {
    const opt: ChoiceOptionDto = {
      id: "zzunknown_format",
      kind: "offer",
      label: "Some unknown offer",
    };
    const decoded = decodeTradeOption(opt);
    expect(decoded.category).toBe("other");
    expect(decoded.label).toBe("Some unknown offer");
    // details should be empty, not throw
    expect(Object.keys(decoded.details)).toHaveLength(0);
  });

  it("mutual_support via payload.kind overrides id prefix", () => {
    // An id that does NOT equal 'ss' but carries kind='mutual_support' in
    // the payload should still be classified as mutual_support.
    const opt: ChoiceOptionDto = {
      id: "ms_custom",
      kind: "offer",
      label: "Mutual Support Variant",
      payload: { kind: "mutual_support", net: 0, their_net: 0 },
    };
    const decoded = decodeTradeOption(opt);
    expect(decoded.category).toBe("mutual_support");
  });
  // --- ask shapes (the partner's notes: Hacan's Trade Convoys deals) ---

  it("decodes paying trade goods for the partner's note (np)", () => {
    const decoded = decodeTradeOption({
      id: "npconvoys:hacan:3",
      kind: "offer",
      label: "pay 3 trade goods for the note convoys:hacan",
      payload: { received_promissory: "convoys:hacan", net: 0, their_net: 0 },
    });
    expect(decoded.category).toBe("promissory");
    expect(decoded.details.receivedNote).toBe("convoys:hacan");
    expect(decoded.details.giveTradeGoods).toBe(3);
    expect(decoded.details.giveCommodities).toBeUndefined();
  });

  it("decodes paying commodities for the partner's note (cp), not as a commodity gift", () => {
    const decoded = decodeTradeOption({
      id: "cpcf:hacan:2",
      kind: "offer",
      label: "pay 2 commodities for the note cf:hacan",
      payload: { received_promissory: "cf:hacan" },
    });
    expect(decoded.category).toBe("promissory");
    expect(decoded.details.receivedNote).toBe("cf:hacan");
    expect(decoded.details.giveCommodities).toBe(2);
  });

  it("decodes our note for their commodities (pc)", () => {
    const decoded = decodeTradeOption({
      id: "pcra:jolnar:4",
      kind: "offer",
      label: "give the note ra:jolnar for 4 commodity",
      payload: { promissory: "ra:jolnar", alias: "ra" },
    });
    expect(decoded.category).toBe("promissory");
    expect(decoded.details.givenNote).toBe("ra:jolnar");
    expect(decoded.details.receiveCommodities).toBe(4);
  });

  it("decodes note for note (nn) and action card for note (cn, Arbiters)", () => {
    const nn = decodeTradeOption({
      id: "nnps:sol>convoys:hacan",
      kind: "offer",
      label: "give the note ps:sol for the note convoys:hacan",
      payload: { promissory: "ps:sol", received_promissory: "convoys:hacan" },
    });
    expect(nn.category).toBe("promissory");
    expect(nn.details.givenNote).toBe("ps:sol");
    expect(nn.details.receivedNote).toBe("convoys:hacan");

    const cn = decodeTradeOption({
      id: "cnrally>cf:hacan",
      kind: "offer",
      label: "give the action card rally for the note cf:hacan",
      payload: { action_card: "rally", promissory: "cf:hacan" },
    });
    expect(cn.category).toBe("promissory");
    expect(cn.details.givenActionCard).toBe("rally");
    expect(cn.details.receivedNote).toBe("cf:hacan");
    expect(cn.details.actionCard).toBeUndefined();
  });
});
