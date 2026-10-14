import { ChoiceOptionDto } from "../protocol/types.ts";
import { getTradePayload } from "./choiceModel.ts";
import { replaceCardIds } from "./tradeNames.ts";

export type TradeCategory =
  | "commodity_swap"
  | "goods_exchange"
  | "promissory"
  | "mutual_support"
  | "other";

export interface DecodedTradeOffer {
  id: string;
  category: TradeCategory;
  /** Shown text: the engine label with card ids replaced by printed names. */
  label: string;
  /** The label as the engine wrote it (ids included); set by `decodeTradeOption`. */
  engineLabel?: string;
  net?: number;
  their_net?: number;
  details: {
    giveCommodities?: number;
    receiveCommodities?: number;
    giveTradeGoods?: number;
    receiveTradeGoods?: number;
    promissoryNote?: string;
    /** A note the proposer hands over in a note-for-X shape (`pc`, `nn`, `cn` ask shapes). */
    givenNote?: string;
    /** A note the proposer asks the partner for (`np`, `cp`, `nn`, `cn` ask shapes). */
    receivedNote?: string;
    /** An action card the proposer hands over in exchange for a note (`cn`). */
    givenActionCard?: string;
    actionCard?: string;
    secretObjective?: string;
    fragment?: string;
    price?: number;
  };
}

/** The note id and price of `{prefix}{note}:{price}`; note ids contain colons, so split on the last. */
function noteAndPrice(rest: string): { note: string; price: number } | null {
  const cut = rest.lastIndexOf(":");
  if (cut <= 0) return null;
  const price = parseInt(rest.slice(cut + 1), 10);
  return Number.isFinite(price) ? { note: rest.slice(0, cut), price } : null;
}

function decodeAskShape(
  id: string,
  payload: Record<string, unknown>,
): { label: string; details: DecodedTradeOffer["details"] } | null {
  const text = (key: string): string | undefined =>
    typeof payload[key] === "string" ? (payload[key] as string) : undefined;
  const prefix = id.slice(0, 2);
  if (prefix === "np" || prefix === "cp") {
    const parsed = noteAndPrice(id.slice(2));
    if (!parsed) return null;
    const note = text("received_promissory") ?? parsed.note;
    return prefix === "np"
      ? {
          label: `Pay ${parsed.price} trade goods for the note ${note}`,
          details: { giveTradeGoods: parsed.price, receivedNote: note, price: parsed.price },
        }
      : {
          label: `Pay ${parsed.price} commodities for the note ${note}`,
          details: { giveCommodities: parsed.price, receivedNote: note, price: parsed.price },
        };
  }
  if (prefix === "pc") {
    const parsed = noteAndPrice(id.slice(2));
    if (!parsed) return null;
    const note = text("promissory") ?? parsed.note;
    return {
      label: `Give the note ${note} for ${parsed.price} commodities`,
      details: { givenNote: note, receiveCommodities: parsed.price, price: parsed.price },
    };
  }
  if (prefix === "nn" || prefix === "cn") {
    const cut = id.indexOf(">");
    if (cut < 0) return null;
    const given = id.slice(2, cut);
    const wanted = text("received_promissory") ?? id.slice(cut + 1);
    return prefix === "nn"
      ? {
          label: `Give the note ${text("promissory") ?? given} for the note ${wanted}`,
          details: { givenNote: text("promissory") ?? given, receivedNote: wanted },
        }
      : {
          label: `Give the action card ${text("action_card") ?? given} for the note ${wanted}`,
          details: { givenActionCard: text("action_card") ?? given, receivedNote: wanted },
        };
  }
  return null;
}

export function decodeTradeOption(opt: ChoiceOptionDto): DecodedTradeOffer {
  const deal = decodeTradeOptionRaw(opt);
  return { ...deal, label: dealLabel(deal), engineLabel: deal.label };
}

function decodeTradeOptionRaw(opt: ChoiceOptionDto): DecodedTradeOffer {
  const id = opt.id;
  const payload = getTradePayload(opt);
  const net = payload.net;
  const their_net = payload.their_net;
  const rawPayload = opt.payload ?? {};

  if (id === "ss" || rawPayload.kind === "mutual_support") {
    return {
      id,
      category: "mutual_support",
      label: opt.label || "Swap Support for the Throne",
      net,
      their_net,
      details: {},
    };
  }

  // 0. Ask shapes: the partner's note for goods or commodities (`np`, `cp`), our note for their
  // commodities (`pc`), note for note (`nn`) or an action card for a note (`cn`). Note ids carry a
  // colon themselves, so none of these may fall through to the generic "{N}:{M}" branches below,
  // which read them as zero-for-zero trades (or, for `cp`/`cn`, as a gift of 0 commodities). They
  // belong on the promissory tab: Hacan's note deals (Trade Convoys, Arbiters) are all of this kind.
  const ask = decodeAskShape(id, rawPayload);
  if (ask) {
    return { id, category: "promissory", label: opt.label || ask.label, net, their_net, details: ask.details };
  }

  // 1. Promissory Notes: prioritize payload.note, payload.price, payload.gift
  if (typeof rawPayload.note === "string" || id.startsWith("pn")) {
    const note =
      typeof rawPayload.note === "string"
        ? rawPayload.note
        : (() => {
            const rest = id.slice(2);
            const lastColon = rest.lastIndexOf(":");
            return lastColon !== -1 ? rest.slice(0, lastColon) : rest;
          })();

    const price =
      typeof rawPayload.price === "number"
        ? rawPayload.price
        : rawPayload.gift === true
          ? 0
          : (() => {
              const rest = id.slice(2);
              const lastColon = rest.lastIndexOf(":");
              return lastColon !== -1 ? parseInt(rest.slice(lastColon + 1), 10) || 0 : 0;
            })();

    return {
      id,
      category: "promissory",
      label: opt.label || `Promissory Note: ${note} for ${price} TG`,
      net,
      their_net,
      details: {
        promissoryNote: note,
        price,
      },
    };
  }

  // 2. Action Cards: prioritize payload.action_card / payload.card
  if (
    typeof rawPayload.action_card === "string" ||
    typeof rawPayload.card === "string" ||
    id.startsWith("ac")
  ) {
    const card =
      typeof rawPayload.action_card === "string"
        ? rawPayload.action_card
        : typeof rawPayload.card === "string"
          ? rawPayload.card
          : id.slice(2).split(":")[0];

    const price =
      typeof rawPayload.price === "number"
        ? rawPayload.price
        : parseInt(id.slice(2).split(":")[1] || "0", 10) || 0;

    return {
      id,
      category: "other",
      label: opt.label || `Action Card: ${card} for ${price} TG`,
      net,
      their_net,
      details: {
        actionCard: card,
        price,
      },
    };
  }

  // 3. Secret Objectives: prioritize payload.secret / payload.secret_objective
  if (
    typeof rawPayload.secret === "string" ||
    typeof rawPayload.secret_objective === "string" ||
    id.startsWith("so")
  ) {
    const secret =
      typeof rawPayload.secret === "string"
        ? rawPayload.secret
        : typeof rawPayload.secret_objective === "string"
          ? rawPayload.secret_objective
          : id.slice(2).split(":")[0];

    const price =
      typeof rawPayload.price === "number"
        ? rawPayload.price
        : parseInt(id.slice(2).split(":")[1] || "0", 10) || 0;

    return {
      id,
      category: "other",
      label: opt.label || `Secret Objective: ${secret} for ${price} TG`,
      net,
      their_net,
      details: {
        secretObjective: secret,
        price,
      },
    };
  }

  // 4. Relic Fragments: prioritize payload.fragment / payload.trait
  if (
    typeof rawPayload.fragment === "string" ||
    typeof rawPayload.trait === "string" ||
    id.startsWith("fr")
  ) {
    const trait =
      typeof rawPayload.fragment === "string"
        ? rawPayload.fragment
        : typeof rawPayload.trait === "string"
          ? rawPayload.trait
          : id.slice(2).split(":")[0];

    const price =
      typeof rawPayload.price === "number"
        ? rawPayload.price
        : parseInt(id.slice(2).split(":")[1] || "0", 10) || 0;

    return {
      id,
      category: "other",
      label: opt.label || `Fragment (${trait}) for ${price} TG`,
      net,
      their_net,
      details: {
        fragment: trait,
        price,
      },
    };
  }

  // 5. Commodity Swaps
  if (typeof rawPayload.swap_commodities === "number" || id.startsWith("cc")) {
    const amount =
      typeof rawPayload.swap_commodities === "number"
        ? rawPayload.swap_commodities
        : parseInt(id.slice(2), 10) || 1;
    return {
      id,
      category: "commodity_swap",
      label: opt.label || `Swap ${amount} commodities each`,
      net,
      their_net,
      details: {
        giveCommodities: amount,
        receiveCommodities: amount,
      },
    };
  }

  // 6. Commodity for TG (ct)
  if (
    (typeof rawPayload.give_commodities === "number" &&
      typeof rawPayload.receive_trade_goods === "number") ||
    id.startsWith("ct")
  ) {
    const give =
      typeof rawPayload.give_commodities === "number"
        ? rawPayload.give_commodities
        : parseInt(id.slice(2).split(":")[0], 10) || 0;
    const want =
      typeof rawPayload.receive_trade_goods === "number"
        ? rawPayload.receive_trade_goods
        : parseInt(id.slice(2).split(":")[1], 10) || 0;
    return {
      id,
      category: "goods_exchange",
      label: opt.label || `Give ${give} commodities for ${want} trade goods`,
      net,
      their_net,
      details: {
        giveCommodities: give,
        receiveTradeGoods: want,
      },
    };
  }

  // 7. TG for Commodity (tc)
  if (
    (typeof rawPayload.give_trade_goods === "number" &&
      typeof rawPayload.receive_commodities === "number") ||
    id.startsWith("tc")
  ) {
    const give =
      typeof rawPayload.give_trade_goods === "number"
        ? rawPayload.give_trade_goods
        : parseInt(id.slice(2).split(":")[0], 10) || 0;
    const want =
      typeof rawPayload.receive_commodities === "number"
        ? rawPayload.receive_commodities
        : parseInt(id.slice(2).split(":")[1], 10) || 0;
    return {
      id,
      category: "goods_exchange",
      label: opt.label || `Give ${give} trade goods for ${want} commodities`,
      net,
      their_net,
      details: {
        giveTradeGoods: give,
        receiveCommodities: want,
      },
    };
  }

  // 8. Commodity Gift (c{N}:0)
  if (id.startsWith("c") && id.includes(":")) {
    const parts = id.slice(1).split(":");
    const give =
      typeof rawPayload.gift_commodities === "number"
        ? rawPayload.gift_commodities
        : parseInt(parts[0], 10) || 0;
    return {
      id,
      category: "commodity_swap",
      label: opt.label || `Gift ${give} commodities`,
      net,
      their_net,
      details: {
        giveCommodities: give,
      },
    };
  }

  // 9. Trade Good Exchange ({N}:{M})
  if (id.includes(":")) {
    const parts = id.split(":");
    const give =
      typeof rawPayload.give_trade_goods === "number"
        ? rawPayload.give_trade_goods
        : parseInt(parts[0], 10) || 0;
    const want =
      typeof rawPayload.receive_trade_goods === "number"
        ? rawPayload.receive_trade_goods
        : parseInt(parts[1], 10) || 0;
    return {
      id,
      category: "goods_exchange",
      label: opt.label || `Give ${give} trade goods for ${want} trade goods`,
      net,
      their_net,
      details: {
        giveTradeGoods: give,
        receiveTradeGoods: want,
      },
    };
  }

  return {
    id,
    category: "other",
    label: opt.label,
    net,
    their_net,
    details: {},
  };
}

/**
 * A deal's label from its decoded terms: the engine label with every note, action card and secret
 * objective id the deal carries replaced by its printed name. Ids the label does not contain leave
 * it as it was, so an option that cannot be decoded keeps the engine's own words.
 */
export function dealLabel(deal: DecodedTradeOffer): string {
  const d = deal.details;
  return replaceCardIds(deal.label, [
    { kind: "note", id: d.promissoryNote },
    { kind: "note", id: d.givenNote },
    { kind: "note", id: d.receivedNote },
    { kind: "action", id: d.actionCard },
    { kind: "action", id: d.givenActionCard },
    { kind: "secret", id: d.secretObjective },
  ]);
}
