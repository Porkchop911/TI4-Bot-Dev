import { ChoiceOptionDto } from "../protocol/types.ts";
import { getTradePayload } from "./choiceModel.ts";

export type TradeCategory =
  | "commodity_swap"
  | "goods_exchange"
  | "promissory"
  | "mutual_support"
  | "other";

export interface DecodedTradeOffer {
  id: string;
  category: TradeCategory;
  label: string;
  net?: number;
  their_net?: number;
  details: {
    giveCommodities?: number;
    receiveCommodities?: number;
    giveTradeGoods?: number;
    receiveTradeGoods?: number;
    promissoryNote?: string;
    actionCard?: string;
    secretObjective?: string;
    fragment?: string;
    price?: number;
  };
}

export function decodeTradeOption(opt: ChoiceOptionDto): DecodedTradeOffer {
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
